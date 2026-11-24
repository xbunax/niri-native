#import <AppKit/AppKit.h>
#import <ApplicationServices/ApplicationServices.h>
#import <dlfcn.h>
#import <errno.h>
#import <unistd.h>

@protocol MoveOperation
- (instancetype)initWithWindows:(NSArray *)windows spaceID:(uint64_t)sid;
- (void)performWithWMBridgeDelegate;
@end

@protocol CreateOperation
- (instancetype)initWithOptions:(unsigned int)options values:(NSDictionary *)values;
- (id)performWithWMBridgeDelegate;
@end

@protocol SpaceOperation
- (instancetype)initWithSpaceID:(uint64_t)sid;
- (instancetype)initWithDisplayIdentifier:(NSString *)display spaceID:(uint64_t)sid;
- (void)performWithWMBridgeDelegate;
@end

@protocol SpaceIDResult
- (uint64_t)spaceID;
@end

static Class operationClass(NSString *name, SEL initializer) {
    Class cls = NSClassFromString(name);
    return [cls instancesRespondToSelector:initializer] &&
        [cls instancesRespondToSelector:@selector(performWithWMBridgeDelegate)] ? cls : Nil;
}

static Class createClass(void) {
    return operationClass(@"SLSBridgedSpaceCreateOperation", @selector(initWithOptions:values:));
}

static Class destroyClass(void) {
    return operationClass(@"SLSBridgedSpaceDestroyOperation", @selector(initWithSpaceID:));
}

static Class focusClass(void) {
    return operationClass(@"SLSBridgedManagedDisplaySetCurrentSpaceOperation", @selector(initWithDisplayIdentifier:spaceID:));
}

static int (*connection)(void);
static CFArrayRef (*copyDisplays)(int);
static CFArrayRef (*copyMembership)(int, int, CFArrayRef);
static void (*performMove)(id);
static void (*legacyMove)(int, CFArrayRef, uint64_t);

static Class bridgedMoveClass(void) {
    Class cls = NSClassFromString(@"SLSBridgedMoveWindowsToManagedSpaceOperation");
    return [cls instancesRespondToSelector:@selector(initWithWindows:spaceID:)] &&
        (performMove || [cls instancesRespondToSelector:@selector(performWithWMBridgeDelegate)]) ? cls : Nil;
}

static BOOL fail(NSString *message) {
    fprintf(stderr, "%s\n", message.UTF8String);
    return NO;
}

static BOOL loadSkyLight(void) {
    void *lib = dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_LAZY | RTLD_LOCAL);
    if (!lib) return fail(@"Cannot load SkyLight");
    connection = dlsym(lib, "SLSMainConnectionID");
    copyDisplays = dlsym(lib, "SLSCopyManagedDisplaySpaces");
    copyMembership = dlsym(lib, "SLSCopySpacesForWindows");
    performMove = dlsym(lib, "SLSPerformAsynchronousBridgedWindowManagementOperation");
    legacyMove = dlsym(lib, "SLSMoveWindowsToManagedSpace");
    return (connection && copyDisplays) || fail(@"Missing SkyLight display API");
}

static NSArray<NSDictionary *> *displays(void) {
    return CFBridgingRelease(copyDisplays(connection()));
}

static NSNumber *spaceID(NSDictionary *space) { return space[@"id64"] ?: space[@"ManagedSpaceID"]; }
static BOOL userSpace(NSDictionary *space) { return [space[@"type"] isEqual:@0]; }

static NSDictionary *displayForSpace(NSArray<NSDictionary *> *all, NSNumber *sid, NSUInteger *index) {
    for (NSDictionary *display in all) {
        NSUInteger i = 0;
        for (NSDictionary *space in display[@"Spaces"]) {
            if ([spaceID(space) isEqual:sid]) {
                if (index) *index = i;
                return display;
            }
            ++i;
        }
    }
    return nil;
}

static NSDictionary *displayNamed(NSArray<NSDictionary *> *all, NSString *name) {
    if (!name.length) return nil;
    for (NSDictionary *display in all) {
        NSString *identifier = display[@"Display Identifier"];
        if (identifier.length && [identifier caseInsensitiveCompare:name] == NSOrderedSame) return display;
    }
    return nil;
}

static NSArray *spaceIDs(NSDictionary *display) {
    NSMutableArray *ids = [NSMutableArray array];
    for (NSDictionary *space in display[@"Spaces"]) [ids addObject:spaceID(space) ?: NSNull.null];
    return ids;
}

static id axGet(AXUIElementRef element, CFStringRef attribute) {
    CFTypeRef value = NULL;
    if (!element || AXUIElementCopyAttributeValue(element, attribute, &value) != kAXErrorSuccess) return nil;
    return CFBridgingRelease(value);
}

static AXUIElementRef ax(id object) { return (__bridge AXUIElementRef)object; }

static id childWithID(AXUIElementRef parent, NSString *identifier) {
    for (id child in axGet(parent, kAXChildrenAttribute))
        if ([axGet(ax(child), CFSTR("AXIdentifier")) isEqual:identifier]) return child;
    return nil;
}

static NSArray *mcDisplays(void) {
    NSRunningApplication *app = [NSRunningApplication runningApplicationsWithBundleIdentifier:@"com.apple.WindowManager"].firstObject;
    if (!app) return nil;
    AXUIElementRef element = AXUIElementCreateApplication(app.processIdentifier);
    NSMutableArray *result = [NSMutableArray array];
    for (id child in axGet(element, kAXChildrenAttribute)) {
        if ([axGet(ax(child), CFSTR("AXIdentifier")) isEqual:@"mc.display"]) [result addObject:child];
    }
    CFRelease(element);
    return result;
}

static id groupForDisplay(NSArray *groups, NSString *name) {
    if ([name isEqualToString:@"Main"] && groups.count == 1) return childWithID(ax(groups.firstObject), @"mc.spaces");
    CGDirectDisplayID did = CGMainDisplayID();
    if (![name isEqualToString:@"Main"]) {
        CFUUIDRef uuid = CFUUIDCreateFromString(NULL, (__bridge CFStringRef)name);
        if (!uuid) return nil;
        did = CGDisplayGetDisplayIDFromUUID(uuid);
        CFRelease(uuid);
        if (!did) return nil;
    }
    CGRect bounds = CGDisplayBounds(did);
    for (id group in groups) {
        id position = axGet(ax(group), kAXPositionAttribute);
        id size = axGet(ax(group), kAXSizeAttribute);
        CGRect frame;
        if (position && size && AXValueGetValue((__bridge AXValueRef)position, kAXValueCGPointType, &frame.origin)
            && AXValueGetValue((__bridge AXValueRef)size, kAXValueCGSizeType, &frame.size)
            && CGRectEqualToRect(frame, bounds)) return childWithID(ax(group), @"mc.spaces");
    }
    return nil;
}

static BOOL toggleMC(void) {
    NSTask *task = [NSTask new];
    task.executableURL = [NSURL fileURLWithPath:@"/usr/bin/open"];
    task.arguments = @[@"/System/Applications/Mission Control.app"];
    NSError *error = nil;
    if (![task launchAndReturnError:&error]) return fail(error.localizedDescription);
    [task waitUntilExit];
    return task.terminationStatus == 0 || fail(@"Could not invoke Mission Control");
}

static BOOL waitFor(BOOL (^predicate)(void)) {
    for (int attempt = 0; attempt < 40; ++attempt) {
        @autoreleasepool { if (predicate()) return YES; }
        usleep(75000);
    }
    return NO;
}

static BOOL nativeCreate(NSArray<NSDictionary *> *before, NSString *targetIdentifier) {
    Class cls = createClass();
    if (!cls) return fail(@"Bridged create API unavailable; use --ax create for Mission Control");
    // kCGSPackagesDisplayIdentifierKey has the same value as the display-list key.
    NSDictionary *values = targetIdentifier ? @{@"type": @0, @"Display Identifier": targetIdentifier} : @{@"type": @0};
    id<CreateOperation> op = [(id<CreateOperation>)[cls alloc] initWithOptions:0 values:values];
    if (!op) return fail(@"Could not initialize the bridged create operation");
    id result = [op performWithWMBridgeDelegate];
    uint64_t sidValue = [result respondsToSelector:@selector(spaceID)] ? [(id<SpaceIDResult>)result spaceID] : 0;
    NSNumber *sid = @(sidValue);
    if (!sidValue || displayForSpace(before, sid, NULL)) return fail(@"SkyLight did not return a new Space ID");
    __block NSString *observedIdentifier = nil;
    BOOL verified = waitFor(^BOOL{
        NSUInteger index;
        NSDictionary *display = displayForSpace(displays(), sid, &index);
        if (!display || !userSpace(display[@"Spaces"][index])) return NO;
        observedIdentifier = display[@"Display Identifier"];
        return !targetIdentifier || [observedIdentifier isEqualToString:targetIdentifier];
    });
    if (!verified && observedIdentifier) return fail([NSString stringWithFormat:
        @"SkyLight created Space %@, observed on display %@ instead of %@; no retry attempted",
        sid, observedIdentifier, targetIdentifier]);
    if (!verified) return fail([NSString stringWithFormat:@"SkyLight returned Space %@, but it was not verified as a managed desktop", sid]);
    puts(sid.stringValue.UTF8String);
    return YES;
}

static BOOL nativeChange(NSDictionary *display, NSNumber *sid, BOOL destroy) {
    Class cls = destroy ? destroyClass() : focusClass();
    if (!cls) return fail(@"Bridged Space API unavailable; use --ax for Mission Control");
    NSString *name = display[@"Display Identifier"];
    id<SpaceOperation> op = [cls alloc];
    op = destroy ? [op initWithSpaceID:sid.unsignedLongLongValue]
                 : [op initWithDisplayIdentifier:name spaceID:sid.unsignedLongLongValue];
    if (!op) return fail(@"Could not initialize the bridged Space operation");
    [op performWithWMBridgeDelegate];
    return waitFor(^BOOL{
        NSArray *after = displays();
        if (destroy) return after.count && !displayForSpace(after, sid, NULL);
        return [spaceID(displayNamed(after, name)[@"Current Space"]) isEqual:sid];
    }) || fail(@"SkyLight Space change could not be verified");
}

static BOOL missionControl(NSString *command, NSDictionary *display, NSNumber *sid, NSUInteger index) {
    if (!AXIsProcessTrusted()) return fail(@"Accessibility permission required (System Settings → Privacy & Security → Accessibility; authorize the host that runs this CLI)");
    NSString *name = display[@"Display Identifier"];
    NSArray *before = spaceIDs(display);
    BOOL create = [command isEqual:@"create"], destroy = [command isEqual:@"destroy"];
    BOOL opened = mcDisplays().count == 0;
    if (opened && !toggleMC()) return NO;
    __block id target = nil;
    BOOL ready = waitFor(^BOOL{
        id group = groupForDisplay(mcDisplays(), name);
        if (!group) return NO;
        if (create) target = childWithID(ax(group), @"mc.spaces.add");
        else {
            id list = childWithID(ax(group), @"mc.spaces.list");
            NSArray *buttons = axGet(ax(list), kAXChildrenAttribute);
            if (buttons.count != before.count || index >= buttons.count) return NO;
            target = buttons[index];
        }
        return target && [axGet(ax(target), kAXRoleAttribute) isEqual:(__bridge NSString *)kAXButtonRole];
    });
    BOOL verified = NO;
    if (!ready) fail(@"Cannot match the WindowManager Mission Control controls to the target display/Space");
    else if (![before isEqual:spaceIDs(displayNamed(displays(), name))]) fail(@"Space order changed; retry after Mission Control settles");
    else {
        AXError error = AXUIElementPerformAction(ax(target), destroy ? CFSTR("AXRemoveDesktop") : kAXPressAction);
        if (error != kAXErrorSuccess) fail([NSString stringWithFormat:@"Mission Control AX action failed (%d)", error]);
        else {
            __block NSNumber *created = nil;
            verified = waitFor(^BOOL{
                NSDictionary *after = displayNamed(displays(), name);
                if (!after) return NO;
                if (destroy) return ![spaceIDs(after) containsObject:sid];
                if (!create) return [spaceID(after[@"Current Space"]) isEqual:sid];
                NSMutableArray *newIDs = [NSMutableArray array];
                for (NSDictionary *space in after[@"Spaces"]) {
                    NSNumber *value = spaceID(space);
                    if (value && userSpace(space) && ![before containsObject:value]) [newIDs addObject:value];
                }
                created = newIDs.count == 1 ? newIDs.firstObject : nil;
                return created != nil;
            });
            if (verified && created) puts(created.stringValue.UTF8String);
            if (!verified) fail(@"AX action was sent, but the resulting Space change could not be verified");
        }
    }
    // AXPress on a Space normally dismisses Mission Control itself. Do not toggle it back on.
    if (opened && mcDisplays().count) toggleMC();
    return verified;
}

static BOOL moveWindow(NSNumber *window, NSNumber *sid) {
    Class cls = bridgedMoveClass();
    if (!copyMembership || (!cls && !legacyMove)) return fail(@"No supported SkyLight move entry point found");
    NSArray *windows = @[window];
    NSArray * (^membership)(void) = ^NSArray *{
        return CFBridgingRelease(copyMembership(connection(), 0x7, (__bridge CFArrayRef)windows));
    };
    NSArray *before = membership();
    if (!before.count) return fail(@"Window ID has no Space membership; use a CGWindowID from this session");
    if ([before containsObject:sid]) return YES;
    if (cls) {
        id<MoveOperation> operation = [(id<MoveOperation>)[cls alloc] initWithWindows:windows spaceID:sid.unsignedLongLongValue];
        if (!operation) return fail(@"Cannot initialize the bridged move operation");
        if (performMove) performMove(operation);
        else [operation performWithWMBridgeDelegate];
    } else {
        legacyMove(connection(), (__bridge CFArrayRef)windows, sid.unsignedLongLongValue);
    }
    return waitFor(^BOOL{ return [membership() containsObject:sid]; }) || fail(@"SkyLight did not move the window to the target Space on this build/session");
}

static NSNumber *parseID(const char *arg) {
    if (!arg[0] || strspn(arg, "0123456789") != strlen(arg)) return nil;
    errno = 0;
    unsigned long long n = strtoull(arg, NULL, 10);
    return n && !errno ? @(n) : nil;
}

int main(int argc, const char **argv) {
    @autoreleasepool {
        BOOL viaAX = argc > 1 && !strcmp(argv[1], "--ax");
        if (viaAX) { --argc; ++argv; }
        NSString *command = argc > 1 ? @(argv[1]) : @"help";
        BOOL list = [command isEqual:@"list"], probe = [command isEqual:@"probe"], create = [command isEqual:@"create"];
        BOOL focus = [command isEqual:@"focus"], destroy = [command isEqual:@"destroy"], move = [command isEqual:@"move"];
        if (!((!viaAX && ((list || probe) && argc == 2)) || (create && (argc == 2 || argc == 3)) || ((focus || destroy) && argc == 3) || (!viaAX && move && argc == 4))) {
            fprintf(stderr, "usage: spaces list | probe | move <window-id> <space-id>\n"
                            "       spaces [--ax] create [display-uuid]\n"
                            "       spaces [--ax] destroy <space-id> | focus <space-id>\n");
            return argc == 1 || [command isEqual:@"--help"] ? 0 : 2;
        }
        if (!loadSkyLight()) return 1;
        if (probe) {
            printf("Bridged create API: %s\nBridged destroy API: %s\nBridged focus API: %s\nBridged move API: %s\nAccessibility (optional --ax): %s\n",
                createClass() ? "available" : "missing", destroyClass() ? "available" : "missing",
                focusClass() ? "available" : "missing", bridgedMoveClass() && copyMembership ? "available" : "missing",
                AXIsProcessTrusted() ? "allowed" : "not allowed");
            return 0;
        }
        NSArray *all = displays();
        if (!all.count) return fail(@"No managed displays in this session") ? 0 : 1;
        if (list) {
            for (NSDictionary *display in all) {
                printf("%s\n", [display[@"Display Identifier"] UTF8String]);
                NSNumber *current = spaceID(display[@"Current Space"]);
                for (NSDictionary *space in display[@"Spaces"])
                    printf("  %s %llu %s\n", [spaceID(space) isEqual:current] ? "*" : " ", spaceID(space).unsignedLongLongValue, userSpace(space) ? "desktop" : "other");
            }
            return 0;
        }
        if (create) {
            NSDictionary *target = argc == 3 ? displayNamed(all, @(argv[2])) : nil;
            if (argc == 3 && !target) return fail(@"Unknown display identifier; see spaces list") ? 0 : 1;
            if (!viaAX) return nativeCreate(all, target[@"Display Identifier"]) ? 0 : 1;
            return missionControl(command, target ?: all.firstObject, nil, 0) ? 0 : 1;
        }
        NSNumber *sid = parseID(argv[move ? 3 : 2]);
        NSNumber *window = move ? parseID(argv[2]) : nil;
        if (!sid || (move && (!window || window.unsignedLongLongValue > UINT32_MAX))) return fail(@"Invalid numeric Space or window ID") ? 0 : 2;
        NSUInteger index;
        NSDictionary *display = displayForSpace(all, sid, &index);
        if (!display) return fail(@"Unknown Space ID; see spaces list") ? 0 : 1;
        if (!focus && !userSpace(display[@"Spaces"][index])) return fail(@"Target must be a normal desktop Space") ? 0 : 1;
        BOOL active = [spaceID(display[@"Current Space"]) isEqual:sid];
        if (focus && active) return 0;
        if (destroy && active) return fail(@"Switch away from the active desktop before destroying it") ? 0 : 1;
        if (move) return moveWindow(window, sid) ? 0 : 1;
        return (viaAX ? missionControl(command, display, sid, index) : nativeChange(display, sid, destroy)) ? 0 : 1;
    }
}
