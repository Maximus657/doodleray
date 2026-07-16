#ifndef DOODLERAY_NETWORK_EXTENSION_BRIDGE_H
#define DOODLERAY_NETWORK_EXTENSION_BRIDGE_H

char *doodleray_ne_start(const char *config_json);
char *doodleray_ne_stop(void);
char *doodleray_ne_status(void);
void doodleray_ne_free(char *value);

#endif
