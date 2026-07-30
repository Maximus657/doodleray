#import "NetworkExtensionBridge.h"

#import <Foundation/Foundation.h>
#import <NetworkExtension/NetworkExtension.h>
#import <ServiceManagement/ServiceManagement.h>
#include <stdlib.h>
#include <string.h>

static NSString *const DoodleRayProviderBundleIdentifier = @"com.doodleray.doodleray.DoodleRayVPN";
static NSString *const DoodleRayManagerDescription = @"DoodleRay";
static NETunnelProviderManager *DoodleRayCachedManager = nil;

typedef void (^DoodleRayFinish)(BOOL success, NSString *status, NSString *message);
typedef void (^DoodleRayArmTimeout)(NSTimeInterval seconds, NSString *message);
typedef BOOL (^DoodleRayIsFinished)(void);

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

static char *DoodleRayCopyAutostartJSON(BOOL success, BOOL supported, BOOL enabled, NSString *message) {
    NSDictionary *payload = @{
        @"success" : @(success),
        @"supported" : @(supported),
        @"enabled" : @(enabled),
        @"message" : message ?: @""
    };
    NSData *data = [NSJSONSerialization dataWithJSONObject:payload options:0 error:nil];
    NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
    return strdup(json.UTF8String ?: "{\"success\":false,\"supported\":false,\"enabled\":false,\"message\":\"Login item error\"}");
}

static NSString *DoodleRayAutostartStatusMessage(SMAppServiceStatus status) {
    if (status == SMAppServiceStatusRequiresApproval) {
        return @"Allow DoodleRay in System Settings > General > Login Items.";
    }
    return @"";
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

static void DoodleRayReply(void *context, DoodleRayNECompletion completion, BOOL success, NSString *status, NSString *message) {
    char *response = DoodleRayCopyJSON(success, status, message);
    if (completion) {
        completion(context, response);
    } else {
        free(response);
    }
}

static void DoodleRayOnMain(dispatch_block_t block) {
    if ([NSThread isMainThread]) {
        block();
    } else {
        dispatch_async(dispatch_get_main_queue(), block);
    }
}

static void DoodleRayBeginOperation(
    void *context,
    DoodleRayNECompletion completion,
    void (^operation)(DoodleRayFinish finish, DoodleRayArmTimeout armTimeout, DoodleRayIsFinished isFinished)
) {
    DoodleRayOnMain(^{
        __block BOOL finished = NO;
        __block NSUInteger timeoutGeneration = 0;

        DoodleRayFinish finish = ^(BOOL success, NSString *status, NSString *message) {
            if (finished) return;
            finished = YES;
            timeoutGeneration += 1;
            DoodleRayReply(context, completion, success, status, message);
        };
        DoodleRayArmTimeout armTimeout = ^(NSTimeInterval seconds, NSString *message) {
            NSUInteger generation = ++timeoutGeneration;
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(seconds * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{
                if (!finished && timeoutGeneration == generation) {
                    NSLog(@"DoodleRay: %@", message);
                    finish(NO, @"invalid", message);
                }
            });
        };
        DoodleRayIsFinished isFinished = ^BOOL {
            return finished;
        };

        operation(finish, armTimeout, isFinished);
    });
}

static void DoodleRayLoadManager(void (^completion)(NETunnelProviderManager *manager, NSString *failure)) {
    if (DoodleRayCachedManager) {
        completion(DoodleRayCachedManager, nil);
        return;
    }

    NSLog(@"DoodleRay: loading Network Extension preferences");
    [NETunnelProviderManager loadAllFromPreferencesWithCompletionHandler:^(NSArray<NETunnelProviderManager *> *managers, NSError *error) {
        if (error) {
            NSLog(@"DoodleRay: failed loading Network Extension preferences: %@", error);
            completion(nil, error.localizedDescription);
            return;
        }

        NSLog(@"DoodleRay: loaded %lu Network Extension manager(s)", (unsigned long)managers.count);
        for (NETunnelProviderManager *manager in managers) {
            NETunnelProviderProtocol *protocol = (NETunnelProviderProtocol *)manager.protocolConfiguration;
            if ([protocol isKindOfClass:[NETunnelProviderProtocol class]] &&
                [protocol.providerBundleIdentifier isEqualToString:DoodleRayProviderBundleIdentifier]) {
                DoodleRayCachedManager = manager;
                completion(manager, nil);
                return;
            }
        }
        completion(nil, nil);
    }];
}

void doodleray_ne_start_async(const char *config_json, void *context, DoodleRayNECompletion completion) {
    @autoreleasepool {
        if (!config_json) {
            DoodleRayReply(context, completion, NO, @"invalid", @"VPN configuration is missing.");
            return;
        }
        NSData *configuration = [NSData dataWithBytes:config_json length:strlen(config_json)];
        if (configuration.length == 0 || configuration.length > 1024 * 1024) {
            DoodleRayReply(context, completion, NO, @"invalid", @"VPN configuration has an invalid size.");
            return;
        }

        DoodleRayBeginOperation(context, completion, ^(DoodleRayFinish finish, DoodleRayArmTimeout armTimeout, DoodleRayIsFinished isFinished) {
            armTimeout(20.0, @"Timed out while loading VPN preferences.");
            DoodleRayLoadManager(^(NETunnelProviderManager *loadedManager, NSString *loadFailure) {
                if (isFinished()) return;
                if (loadFailure) {
                    finish(NO, @"invalid", loadFailure);
                    return;
                }

                NETunnelProviderManager *manager = loadedManager ?: [[NETunnelProviderManager alloc] init];
                armTimeout(20.0, @"Timed out while preparing the VPN profile.");
                [manager loadFromPreferencesWithCompletionHandler:^(NSError *prepareError) {
                    if (isFinished()) return;
                    if (prepareError) {
                        NSLog(@"DoodleRay: failed preparing Network Extension preferences: %@", prepareError);
                        finish(NO, @"invalid", prepareError.localizedDescription);
                        return;
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

                    NSLog(@"DoodleRay: saving Network Extension preferences");
                    armTimeout(90.0, @"Timed out while saving VPN preferences.");
                    [manager saveToPreferencesWithCompletionHandler:^(NSError *saveError) {
                        if (isFinished()) return;
                        if (saveError) {
                            NSLog(@"DoodleRay: failed saving Network Extension preferences: %@", saveError);
                            finish(NO, @"invalid", saveError.localizedDescription);
                            return;
                        }

                        NSLog(@"DoodleRay: reloading Network Extension preferences");
                        armTimeout(20.0, @"Timed out while reloading VPN preferences.");
                        [manager loadFromPreferencesWithCompletionHandler:^(NSError *reloadError) {
                            if (isFinished()) return;
                            if (reloadError) {
                                NSLog(@"DoodleRay: failed reloading Network Extension preferences: %@", reloadError);
                                finish(NO, @"invalid", reloadError.localizedDescription);
                                return;
                            }

                            DoodleRayCachedManager = manager;
                            NSError *startError = nil;
                            NSLog(@"DoodleRay: starting packet tunnel");
                            BOOL started = [(NETunnelProviderSession *)manager.connection
                                startTunnelWithOptions:@{ @"xrayConfig" : configuration }
                                andReturnError:&startError];
                            NEVPNStatus status = manager.connection.status;
                            if (!started || startError) {
                                NSLog(@"DoodleRay: failed starting packet tunnel: %@", startError);
                                finish(NO, DoodleRayStatusName(status), startError.localizedDescription);
                                return;
                            }
                            NSLog(@"DoodleRay: packet tunnel start requested");
                            finish(YES, DoodleRayStatusName(status), @"");
                        }];
                    }];
                }];
            });
        });
    }
}

void doodleray_ne_stop_async(void *context, DoodleRayNECompletion completion) {
    DoodleRayBeginOperation(context, completion, ^(DoodleRayFinish finish, DoodleRayArmTimeout armTimeout, DoodleRayIsFinished isFinished) {
        armTimeout(20.0, @"Timed out while loading VPN preferences.");
        DoodleRayLoadManager(^(NETunnelProviderManager *manager, NSString *failure) {
            if (isFinished()) return;
            if (failure) {
                finish(NO, @"invalid", failure);
                return;
            }
            if (!manager) {
                finish(YES, @"disconnected", @"");
                return;
            }
            [manager.connection stopVPNTunnel];
            finish(YES, DoodleRayStatusName(manager.connection.status), @"");
        });
    });
}

void doodleray_ne_status_async(void *context, DoodleRayNECompletion completion) {
    DoodleRayBeginOperation(context, completion, ^(DoodleRayFinish finish, DoodleRayArmTimeout armTimeout, DoodleRayIsFinished isFinished) {
        armTimeout(20.0, @"Timed out while loading VPN preferences.");
        DoodleRayLoadManager(^(NETunnelProviderManager *manager, NSString *failure) {
            if (isFinished()) return;
            if (failure) {
                finish(NO, @"invalid", failure);
                return;
            }
            finish(YES, manager ? DoodleRayStatusName(manager.connection.status) : @"disconnected", @"");
        });
    });
}

void doodleray_ne_stop_cached(void) {
    DoodleRayOnMain(^{
        [DoodleRayCachedManager.connection stopVPNTunnel];
    });
}

char *doodleray_app_group_container_path(void) {
    NSURL *url = [[NSFileManager defaultManager]
        containerURLForSecurityApplicationGroupIdentifier:@"group.com.doodleray.doodleray"];
    return url.path.length > 0 ? strdup(url.path.UTF8String) : NULL;
}

char *doodleray_autostart_status(void) {
    @autoreleasepool {
        if (@available(macOS 13.0, *)) {
            SMAppService *service = SMAppService.mainAppService;
            SMAppServiceStatus status = service.status;
            return DoodleRayCopyAutostartJSON(
                YES,
                YES,
                status == SMAppServiceStatusEnabled,
                DoodleRayAutostartStatusMessage(status)
            );
        }
        return DoodleRayCopyAutostartJSON(
            NO,
            NO,
            NO,
            @"Launch at startup requires macOS 13 or later."
        );
    }
}

char *doodleray_autostart_set_enabled(int enabled) {
    @autoreleasepool {
        if (@available(macOS 13.0, *)) {
            SMAppService *service = SMAppService.mainAppService;
            SMAppServiceStatus before = service.status;
            if ((enabled != 0 && before == SMAppServiceStatusEnabled) ||
                (enabled == 0 && before == SMAppServiceStatusNotRegistered)) {
                return DoodleRayCopyAutostartJSON(YES, YES, enabled != 0, @"");
            }

            NSError *error = nil;
            BOOL succeeded = enabled != 0
                ? [service registerAndReturnError:&error]
                : [service unregisterAndReturnError:&error];
            SMAppServiceStatus after = service.status;
            BOOL isEnabled = after == SMAppServiceStatusEnabled;
            NSString *message = error.localizedDescription ?: DoodleRayAutostartStatusMessage(after);
            return DoodleRayCopyAutostartJSON(succeeded, YES, isEnabled, message);
        }
        return DoodleRayCopyAutostartJSON(
            NO,
            NO,
            NO,
            @"Launch at startup requires macOS 13 or later."
        );
    }
}

void doodleray_ne_free(char *value) {
    free(value);
}
