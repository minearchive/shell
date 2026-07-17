# TODO

## Rendering
- [ ] Redraw Only updated Area

## Audio
- [ ] status provider
- [ ] OSD

## Battery
- [ ] status provider
- [ ] alert

## IPC
- [ ] mapping all niri events
- [ ] mapping all hyprland events
- [ ] IPC events

## DBUS
- [ ] kde-connect
  - [x] Handling Connect/Disconnect
  - [ ] Notification mapping, tagging it from which device
- [x] notification
- [x] cloudflare-warp 
- [ ] network stats, event

## Monitor
- [ ] status provider
- [ ] mirroring in niri
- [ ] system resources

## Shell
- [ ] set keyboard interactivity to None at creation, drop the pointer Enter hack
- [ ] keyboard input, only if the bar ever needs it (needs get_keyboard_with_repeat)

# Component
- [ ] Material3 Main Component
  - [x] Button
  - [ ] Slider
  - [ ] Textbox
    - [x] keyboard input, cursor, paste
    - [ ] selection range, cheapest before more edit ops land (state -> anchor + cursor)
    - [ ] copy/cut, needs selection first
    - [ ] IME / preedit, adapters only since Commit is the one insertion path
  - [ ] DatePicker
  - [ ] Chips
