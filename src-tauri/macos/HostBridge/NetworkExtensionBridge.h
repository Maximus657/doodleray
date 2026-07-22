#ifndef DOODLERAY_NETWORK_EXTENSION_BRIDGE_H
#define DOODLERAY_NETWORK_EXTENSION_BRIDGE_H

typedef void (*DoodleRayNECompletion)(void *context, char *response);

void doodleray_ne_start_async(const char *config_json, void *context, DoodleRayNECompletion completion);
void doodleray_ne_stop_async(void *context, DoodleRayNECompletion completion);
void doodleray_ne_status_async(void *context, DoodleRayNECompletion completion);
void doodleray_ne_stop_cached(void);
void doodleray_ne_free(char *value);

#endif
