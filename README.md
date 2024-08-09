# i3bar-river

This is a port of `i3bar` for wlroots-based window managers. Tags/workspaces are implemented for [river](https://github.com/riverwm/river) and [hyprland](https://github.com/hyprwm/Hyprland).

## i3bar compatibility

I've tested [`i3status-rs`](https://github.com/greshake/i3status-rust), [`bumblebee-status`](https://github.com/tobi-wan-kenobi/bumblebee-status) and [`py3status`](https://github.com/ultrabug/py3status) and everything seems usable.

A list of things that are missing (for now):
- `border[_top|_right|_bottom|_left]`
- Click events lack some info (IDK if anyone actually relies on `x`, `y`, `width`, etc.)
- Tray icons

## Features

- `river` support (obviously)
- `short_text` switching is "progressive" (see https://github.com/i3/i3/issues/4113)
- Support for rounded corners
- Show/hide with `pkill -SIGUSR1 i3bar-river`

## Installation

[![Packaging status](https://repology.org/badge/vertical-allrepos/i3bar-river.svg)](https://repology.org/project/i3bar-river/versions)

### From Source

External dependencies: `libpango1.0-dev`.

```
cargo install --locked i3bar-river
```

## Configuration

Add this to the end of your river init script:

```
riverctl spawn i3bar-river
```

The configuration file should be stored in `$XDG_CONFIG_HOME/i3bar-river/config.toml` or `~/.config/i3bar-river/config.toml`.

The default configuration (every parameter is optional):

```toml
# The status generator command.
# Optional: with no status generator the bar will display only tags and layout name.
# command = "your command here"

# Colors
background = "#282828ff"
color = "#ffffffff"
separator = "#9a8a62ff"
tag_fg = "#d79921ff"
tag_bg = "#282828ff"
tag_focused_fg = "#1d2021ff"
tag_focused_bg = "#689d68ff"
tag_urgent_fg = "#282828ff"
tag_urgent_bg = "#cc241dff"
tag_inactive_fg = "#d79921ff"
tag_inactive_bg = "#282828ff"

# The font and various sizes
font = "monospace 10"
height = 24
margin_top = 0
margin_bottom = 0
margin_left = 0
margin_right = 0
separator_width = 2.0
tags_r = 0.0
tags_padding = 25.0
tags_margin = 0.0
blocks_r = 0.0
blocks_overlap = 0.0

# Misc
position = "top" # either "top" or "bottom"
layer = "top" # one of "top", "overlay", "bottom" or "background"
hide_inactive_tags = true
invert_touchpad_scrolling = true
show_tags = true
show_layout_name = true
blend = true # whether tags/blocks colors should blend with bar's background
show_mode = true

# WM-specific options
[wm.river]
max_tag = 9 # Show only the first nine tags

# Per output overrides
# [output.your-output-name]
# right now only "enable" option is available
# enable = false
#
# You can have any number of overrides
# [output.eDP-1]
# enable = false
```

## How progressive short mode and rounded corners work

Some status bar generators (such as `i3status-rs`) use more than one "json block" per logical block
to implement, for example, buttons.

_Short text management and corner rounding are performed on per-logical-block basis._

`i3bar-river` defines a logical block as a continuous series of "json blocks" with the same `name`.
Also, only the last "json block" is allowed to have a non zero (or absent) `separator_block_width`,
all the other "json blocks" should explicitly set it to zero. If you think this definition is not
the best one, feel free to open a new github issue.

## `blocks_overlap` option

Sometimes `pango` lives a gap between "powerline separators" and the blocks (see https://github.com/greshake/i3status-rust/issues/246#issuecomment-1086753440). In this case, you can set `blocks_overlap` option to number of pixels you want your blocks to overlap. Usually, `1` is a good choice.

## Showcase (with i3status-rs)

### Native separators

![Native separators demo](../assets/native_demo.png?raw=true)

`i3bar-river`

```toml
font = "JetBrainsMono Nerd Font 10"
height = 20
command = "i3status-rs"
```

`i3status-rs`

```toml
[theme]
theme = "native"
[theme.overrides]
idle_fg = "#ebdbb2"
info_fg = "#458588"
good_fg = "#8ec07c"
warning_fg = "#fabd2f"
critical_fg = "#fb4934"
```

### Powerline separators

![Powerline separators demo](../assets/powerline_demo.png?raw=true)

`i3bar-river`

```toml
font = "JetBrainsMono Nerd Font 10"
height = 20
command = "i3status-rs"
```

`i3status-rs`

```toml
[theme]
theme = "slick"
```

### Rounded corners

![Rounded corners demo](../assets/rounded_corners_demo.png?raw=true)

`i3bar-river`

```toml
font = "JetBrainsMono Nerd Font 10"
height = 20
separator_width = 0
tags_r = 6
blocks_r = 6
command = "i3status-rs"
```

`i3status-rs`

```toml
[theme]
theme = "slick"
[theme.overrides]
separator = "native"
alternating_tint_bg = "none"
alternating_tint_fg = "none"
```
