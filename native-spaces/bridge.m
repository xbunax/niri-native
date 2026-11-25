// Submission-only bridge. The Rust actor verifies mutations against SkyLight.
#import <Foundation/Foundation.h>
#import <dlfcn.h>
#import <stdint.h>

@protocol RiftCreateSpace
- (instancetype)initWithOptions:(unsigned int)options values:(NSDictionary *)values;
- (id)performWithWMBridgeDelegate;
@end

@protocol RiftSpaceResult
- (uint64_t)spaceID;
@end

@protocol RiftSpaceOperation
- (instancetype)initWithSpaceID:(uint64_t)space;
- (instancetype)initWithSpaces:(NSArray *)spaces;
- (instancetype)initWithDisplayIdentifier:(NSString *)display spaceID:(uint64_t)space;
- (instancetype)initWithWindows:(NSArray *)windows spaceID:(uint64_t)space;
- (void)performWithWMBridgeDelegate;
@end

static Class operation(NSString *name, SEL initializer) {
    Class cls = NSClassFromString(name);
    return [cls instancesRespondToSelector:initializer] ? cls : Nil;
}

uint64_t rift_native_space_create(const char *display) {
    @autoreleasepool {
        @try {
            Class cls = operation(@"SLSBridgedSpaceCreateOperation", @selector(initWithOptions:values:));
            if (![cls instancesRespondToSelector:@selector(performWithWMBridgeDelegate)]) return 0;
            NSMutableDictionary *values = [@{@"type": @0} mutableCopy];
            if (display) values[@"Display Identifier"] = @(display);
            id<RiftCreateSpace> op = [(id<RiftCreateSpace>)[cls alloc] initWithOptions:0 values:values];
            id result = [op performWithWMBridgeDelegate];
            return [result respondsToSelector:@selector(spaceID)] ? [(id<RiftSpaceResult>)result spaceID] : 0;
        } @catch (NSException *exception) {
            NSLog(@"Rift native Space create: %@", exception);
            return 0;
        }
    }
}

int rift_native_space_destroy(uint64_t space) {
    @autoreleasepool {
        @try {
            Class cls = operation(@"SLSBridgedSpaceDestroyOperation", @selector(initWithSpaceID:));
            if (![cls instancesRespondToSelector:@selector(performWithWMBridgeDelegate)]) return 0;
            id<RiftSpaceOperation> op = [(id<RiftSpaceOperation>)[cls alloc] initWithSpaceID:space];
            if (!op) return 0;
            [op performWithWMBridgeDelegate];
            return 1;
        } @catch (NSException *exception) {
            NSLog(@"Rift native Space destroy: %@", exception);
            return 0;
        }
    }
}

int rift_native_space_focus(const char *display, uint64_t space) {
    @autoreleasepool {
        @try {
            Class hide = operation(@"SLSBridgedHideSpacesOperation", @selector(initWithSpaces:));
            if (![hide instancesRespondToSelector:@selector(performWithWMBridgeDelegate)]) return 0;
            Class cls = operation(@"SLSBridgedManagedDisplaySetCurrentSpaceOperation", @selector(initWithDisplayIdentifier:spaceID:));
            if (![cls instancesRespondToSelector:@selector(performWithWMBridgeDelegate)]) return 0;
            id<RiftSpaceOperation> op = [(id<RiftSpaceOperation>)[cls alloc] initWithDisplayIdentifier:@(display) spaceID:space];
            if (!op) return 0;
            [op performWithWMBridgeDelegate];
            return 1;
        } @catch (NSException *exception) {
            NSLog(@"Rift native Space focus: %@", exception);
            return 0;
        }
    }
}

int rift_native_space_hide(uint64_t space) {
    @autoreleasepool {
        @try {
            Class cls = operation(@"SLSBridgedHideSpacesOperation", @selector(initWithSpaces:));
            if (![cls instancesRespondToSelector:@selector(performWithWMBridgeDelegate)]) return 0;
            id<RiftSpaceOperation> op = [(id<RiftSpaceOperation>)[cls alloc] initWithSpaces:@[@(space)]];
            if (!op) return 0;
            [op performWithWMBridgeDelegate];
            return 1;
        } @catch (NSException *exception) {
            NSLog(@"Rift native Space hide: %@", exception);
            return 0;
        }
    }
}

int rift_native_space_move(const uint32_t *windows, size_t count, uint64_t space) {
    @autoreleasepool {
        @try {
            Class cls = operation(@"SLSBridgedMoveWindowsToManagedSpaceOperation", @selector(initWithWindows:spaceID:));
            if (!cls) return 0;
            void (*perform)(id) = dlsym(RTLD_DEFAULT, "SLSPerformAsynchronousBridgedWindowManagementOperation");
            if (!perform && ![cls instancesRespondToSelector:@selector(performWithWMBridgeDelegate)]) return 0;
            NSMutableArray *ids = [NSMutableArray arrayWithCapacity:count];
            for (size_t i = 0; i < count; ++i) [ids addObject:@(windows[i])];
            id<RiftSpaceOperation> op = [(id<RiftSpaceOperation>)[cls alloc] initWithWindows:ids spaceID:space];
            if (!op) return 0;
            if (perform) perform(op);
            else [op performWithWMBridgeDelegate];
            return 1;
        } @catch (NSException *exception) {
            NSLog(@"Rift native Space move: %@", exception);
            return 0;
        }
    }
}
