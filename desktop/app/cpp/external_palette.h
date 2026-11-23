// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

// Read-only Hive palette from $XDG_CONFIG_HOME/nectarlink/noctalia-colors.json.
// Enabling the palette is an ordinary Preferences setting, not a sidecar file.
// Must be registered before QML is loaded. No network or shell commands.
void register_external_palette();
