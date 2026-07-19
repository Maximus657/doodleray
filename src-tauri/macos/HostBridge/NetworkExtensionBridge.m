#import "NetworkExtensionBridge.h"

#import <Foundation/Foundation.h>
#import <NetworkExtension/NetworkExtension.h>
#include <stdlib.h>
#include <string.h>

static NSString *const DoodleRayProviderBundleIdentifier = @"com.doodleray.doodleray.DoodleRayVPN";
static NSString *const DoodleRayManagerDescription = @"DoodleRay VPN";
static NSTimeInterval const DoodleRayPreferenceTimeout = 60.0;
static NETunnelProviderManager *DoodleRayCachedManager = nil;

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

        dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
        __block NSArray<NETunnelProviderManager *> *loadedManagers = nil;
        __block NSError *loadError = nil;
        [NETunnelProviderManager loadAllFromPreferencesWithCompletionHandler:^(NSArray<NETunnelProviderManager *> *managers, NSError *error) {
            loadedManagers = managers;
            loadError = error;
            dispatch_semaphore_signal(semaphore);
        }];
        if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(DoodleRayPreferenceTimeout * NSEC_PER_SEC))) != 0) {
            if (failure) *failure = @"Timed out while loading VPN preferences.";
            return nil;
        }
        if (loadError) {
            if (failure) *failure = loadError.localizedDescription;
            return nil;
        }
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
    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    __block NSError *saveError = nil;
    [manager saveToPreferencesWithCompletionHandler:^(NSError *error) {
        saveError = error;
        dispatch_semaphore_signal(semaphore);
    }];
    if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(DoodleRayPreferenceTimeout * NSEC_PER_SEC))) != 0) {
        if (failure) *failure = @"Timed out while saving VPN preferences.";
        return NO;
    }
    if (saveError) {
        if (failure) *failure = saveError.localizedDescription;
        return NO;
    }

    semaphore = dispatch_semaphore_create(0);
    __block NSError *reloadError = nil;
    [manager loadFromPreferencesWithCompletionHandler:^(NSError *error) {
        reloadError = error;
        dispatch_semaphore_signal(semaphore);
    }];
    if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(DoodleRayPreferenceTimeout * NSEC_PER_SEC))) != 0) {
        if (failure) *failure = @"Timed out while reloading VPN preferences.";
        return NO;
    }
    if (reloadError) {
        if (failure) *failure = reloadError.localizedDescription;
        return NO;
    }
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

        NETunnelProviderProtocol *protocol = [[NETunnelProviderProtocol alloc] init];
        protocol.providerBundleIdentifier = DoodleRayProviderBundleIdentifier;
        protocol.serverAddress = DoodleRayManagerDescription;
        protocol.disconnectOnSleep = NO;
        protocol.providerConfiguration = @{ @"configurationVersion" : @1 };
        manager.protocolConfiguration = protocol;
        manager.localizedDescription = DoodleRayManagerDescription;
        manager.enabled = YES;
        manager.onDemandEnabled = NO;

        if (!DoodleRaySaveManager(manager, &failure)) {
            return DoodleRayCopyJSON(NO, @"invalid", failure);
        }
        DoodleRayCachedManager = manager;

        NSError *startError = nil;
        BOOL started = [(NETunnelProviderSession *)manager.connection
            startTunnelWithOptions:@{ @"xrayConfig" : configuration }
            andReturnError:&startError];
        if (!started || startError) {
            return DoodleRayCopyJSON(NO, DoodleRayStatusName(manager.connection.status), startError.localizedDescription);
        }
        return DoodleRayCopyJSON(YES, DoodleRayStatusName(manager.connection.status), @"");
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
        [manager.connection stopVPNTunnel];
        return DoodleRayCopyJSON(YES, DoodleRayStatusName(manager.connection.status), @"");
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
        return DoodleRayCopyJSON(YES, DoodleRayStatusName(manager.connection.status), @"");
    }
}

void doodleray_ne_free(char *value) {
    free(value);
}
