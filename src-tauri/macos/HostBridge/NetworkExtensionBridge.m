#import "NetworkExtensionBridge.h"

#import <Foundation/Foundation.h>
#import <NetworkExtension/NetworkExtension.h>
#include <stdlib.h>
#include <string.h>

static NSString *const DoodleRayProviderBundleIdentifier = @"com.doodleray.doodleray.DoodleRayVPN";
static NSString *const DoodleRayManagerDescription = @"DoodleRay VPN";
static NSTimeInterval const DoodleRayPreferenceTimeout = 15.0;
static NETunnelProviderManager *DoodleRayCachedManager = nil;

static BOOL DoodleRayRunOnMain(dispatch_block_t block, NSString **failure, NSString *timeoutMessage) {
    if ([NSThread isMainThread]) {
        block();
        return YES;
    }

    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    dispatch_async(dispatch_get_main_queue(), ^{
        block();
        dispatch_semaphore_signal(semaphore);
    });
    if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(DoodleRayPreferenceTimeout * NSEC_PER_SEC))) != 0) {
        if (failure) *failure = timeoutMessage;
        return NO;
    }
    return YES;
}

static char *DoodleRayCopyJSON(BOOL success, NSString *status, NSString *message) {
    NSDictionary *payload = @{
        @"success" : @(success),
        @"status" : status ?: @"unknown",
        @"message" : message ?: @""
    };
    NSData *data = [NSJSONSerialization dataWithJSONObject:payload options:0 error:nil];
    NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
    return strdup(json.UTF8String ?: "{\"success\":false,\"status\":\"unknown\",\"message\":\"Network Extension error\"}");
}

static NSString *DoodleRayStatusName(NEVPNStatus status) {
    switch (status) {
        case NEVPNStatusInvalid: return @"invalid";
        case NEVPNStatusDisconnected: return @"disconnected";
        case NEVPNStatusConnecting: return @"connecting";
        case NEVPNStatusConnected: return @"connected";
        case NEVPNStatusReasserting: return @"reasserting";
        case NEVPNStatusDisconnecting: return @"disconnecting";
    }
    return @"unknown";
}

static NETunnelProviderManager *DoodleRayLoadManager(NSString **failure) {
    @synchronized ([NETunnelProviderManager class]) {
        if (DoodleRayCachedManager) return DoodleRayCachedManager;

        if ([NSThread isMainThread]) {
            if (failure) *failure = @"VPN preferences cannot be loaded on the main thread.";
            return nil;
        }

        dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
        __block NSArray<NETunnelProviderManager *> *loadedManagers = nil;
        __block NSError *loadError = nil;
        NSLog(@"DoodleRay VPN: loading Network Extension preferences");
        dispatch_async(dispatch_get_main_queue(), ^{
            [NETunnelProviderManager loadAllFromPreferencesWithCompletionHandler:^(NSArray<NETunnelProviderManager *> *managers, NSError *error) {
                loadedManagers = managers;
                loadError = error;
                dispatch_semaphore_signal(semaphore);
            }];
        });
        if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(DoodleRayPreferenceTimeout * NSEC_PER_SEC))) != 0) {
            NSLog(@"DoodleRay VPN: timed out loading Network Extension preferences");
            if (failure) *failure = @"Timed out while loading VPN preferences.";
            return nil;
        }
        if (loadError) {
            NSLog(@"DoodleRay VPN: failed loading Network Extension preferences: %@", loadError);
            if (failure) *failure = loadError.localizedDescription;
            return nil;
        }
        NSLog(@"DoodleRay VPN: loaded %lu Network Extension manager(s)", (unsigned long)loadedManagers.count);
        for (NETunnelProviderManager *manager in loadedManagers) {
            NETunnelProviderProtocol *protocol = (NETunnelProviderProtocol *)manager.protocolConfiguration;
            if ([protocol isKindOfClass:[NETunnelProviderProtocol class]] &&
                [protocol.providerBundleIdentifier isEqualToString:DoodleRayProviderBundleIdentifier]) {
                DoodleRayCachedManager = manager;
                return manager;
            }
        }
        return nil;
    }
    return nil;
}

static BOOL DoodleRaySaveManager(NETunnelProviderManager *manager, NSString **failure) {
    if ([NSThread isMainThread]) {
        if (failure) *failure = @"VPN preferences cannot be saved on the main thread.";
        return NO;
    }

    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    __block NSError *saveError = nil;
    NSLog(@"DoodleRay VPN: saving Network Extension preferences");
    dispatch_async(dispatch_get_main_queue(), ^{
        [manager saveToPreferencesWithCompletionHandler:^(NSError *error) {
            saveError = error;
            dispatch_semaphore_signal(semaphore);
        }];
    });
    if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(DoodleRayPreferenceTimeout * NSEC_PER_SEC))) != 0) {
        NSLog(@"DoodleRay VPN: timed out saving Network Extension preferences");
        if (failure) *failure = @"Timed out while saving VPN preferences.";
        return NO;
    }
    if (saveError) {
        NSLog(@"DoodleRay VPN: failed saving Network Extension preferences: %@", saveError);
        if (failure) *failure = saveError.localizedDescription;
        return NO;
    }

    semaphore = dispatch_semaphore_create(0);
    __block NSError *reloadError = nil;
    NSLog(@"DoodleRay VPN: reloading Network Extension preferences");
    dispatch_async(dispatch_get_main_queue(), ^{
        [manager loadFromPreferencesWithCompletionHandler:^(NSError *error) {
            reloadError = error;
            dispatch_semaphore_signal(semaphore);
        }];
    });
    if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(DoodleRayPreferenceTimeout * NSEC_PER_SEC))) != 0) {
        NSLog(@"DoodleRay VPN: timed out reloading Network Extension preferences");
        if (failure) *failure = @"Timed out while reloading VPN preferences.";
        return NO;
    }
    if (reloadError) {
        NSLog(@"DoodleRay VPN: failed reloading Network Extension preferences: %@", reloadError);
        if (failure) *failure = reloadError.localizedDescription;
        return NO;
    }
    NSLog(@"DoodleRay VPN: Network Extension preferences are ready");
    return YES;
}

char *doodleray_ne_start(const char *config_json) {
    @autoreleasepool {
        if (!config_json) {
            return DoodleRayCopyJSON(NO, @"invalid", @"VPN configuration is missing.");
        }
        NSData *configuration = [NSData dataWithBytes:config_json length:strlen(config_json)];
        if (configuration.length == 0 || configuration.length > 1024 * 1024) {
            return DoodleRayCopyJSON(NO, @"invalid", @"VPN configuration has an invalid size.");
        }

        NSString *failure = nil;
        NETunnelProviderManager *manager = DoodleRayLoadManager(&failure);
        if (!manager && failure) {
            return DoodleRayCopyJSON(NO, @"invalid", failure);
        }
        if (!manager) {
            manager = [[NETunnelProviderManager alloc] init];
        }

        if (!DoodleRayRunOnMain(^{
            NETunnelProviderProtocol *protocol = [[NETunnelProviderProtocol alloc] init];
            protocol.providerBundleIdentifier = DoodleRayProviderBundleIdentifier;
            protocol.serverAddress = DoodleRayManagerDescription;
            protocol.disconnectOnSleep = NO;
            protocol.providerConfiguration = @{ @"configurationVersion" : @1 };
            manager.protocolConfiguration = protocol;
            manager.localizedDescription = DoodleRayManagerDescription;
            manager.enabled = YES;
            manager.onDemandEnabled = NO;
        }, &failure, @"Timed out while configuring the VPN profile.")) {
            return DoodleRayCopyJSON(NO, @"invalid", failure);
        }

        if (!DoodleRaySaveManager(manager, &failure)) {
            return DoodleRayCopyJSON(NO, @"invalid", failure);
        }
        @synchronized ([NETunnelProviderManager class]) {
            DoodleRayCachedManager = manager;
        }

        __block NSError *startError = nil;
        __block BOOL started = NO;
        __block NEVPNStatus status = NEVPNStatusInvalid;
        NSLog(@"DoodleRay VPN: starting packet tunnel");
        if (!DoodleRayRunOnMain(^{
            started = [(NETunnelProviderSession *)manager.connection
                startTunnelWithOptions:@{ @"xrayConfig" : configuration }
                andReturnError:&startError];
            status = manager.connection.status;
        }, &failure, @"Timed out while starting the VPN tunnel.")) {
            return DoodleRayCopyJSON(NO, @"invalid", failure);
        }
        if (!started || startError) {
            NSLog(@"DoodleRay VPN: failed starting packet tunnel: %@", startError);
            return DoodleRayCopyJSON(NO, DoodleRayStatusName(status), startError.localizedDescription);
        }
        NSLog(@"DoodleRay VPN: packet tunnel start requested");
        return DoodleRayCopyJSON(YES, DoodleRayStatusName(status), @"");
    }
}

char *doodleray_ne_stop(void) {
    @autoreleasepool {
        NSString *failure = nil;
        NETunnelProviderManager *manager = DoodleRayLoadManager(&failure);
        if (!manager) {
            if (failure) return DoodleRayCopyJSON(NO, @"invalid", failure);
            return DoodleRayCopyJSON(YES, @"disconnected", @"");
        }
        __block NEVPNStatus status = NEVPNStatusInvalid;
        if (!DoodleRayRunOnMain(^{
            [manager.connection stopVPNTunnel];
            status = manager.connection.status;
        }, &failure, @"Timed out while stopping the VPN tunnel.")) {
            return DoodleRayCopyJSON(NO, @"invalid", failure);
        }
        return DoodleRayCopyJSON(YES, DoodleRayStatusName(status), @"");
    }
}

char *doodleray_ne_status(void) {
    @autoreleasepool {
        NSString *failure = nil;
        NETunnelProviderManager *manager = DoodleRayLoadManager(&failure);
        if (!manager) {
            if (failure) return DoodleRayCopyJSON(NO, @"invalid", failure);
            return DoodleRayCopyJSON(YES, @"disconnected", @"");
        }
        __block NEVPNStatus status = NEVPNStatusInvalid;
        if (!DoodleRayRunOnMain(^{
            status = manager.connection.status;
        }, &failure, @"Timed out while reading VPN status.")) {
            return DoodleRayCopyJSON(NO, @"invalid", failure);
        }
        return DoodleRayCopyJSON(YES, DoodleRayStatusName(status), @"");
    }
}

void doodleray_ne_stop_cached(void) {
    @autoreleasepool {
        __block NETunnelProviderManager *manager = nil;
        @synchronized ([NETunnelProviderManager class]) {
            manager = DoodleRayCachedManager;
        }
        if (manager) {
            dispatch_async(dispatch_get_main_queue(), ^{
                [manager.connection stopVPNTunnel];
            });
        }
    }
}

void doodleray_ne_free(char *value) {
    free(value);
}
