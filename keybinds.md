# keybinds

## TODO

- i kinda want variable scoping but that's hard so don't bother with it for now
- HSC1 keybind sets are implemented by having a boolean variable for each and a folder with a guard

## variables

- only global variables for now
- variable types: bool, grip, mask, multiplicity
    - maybe nullable? so we can do fallbacks with "or" or "??"
- builtin variables
    - hovered_grip
    - mouse_left, mouse_right, mouse_middle for whether that mouse button is down
    - key_a, ..., key_rshift for whether that key is down, including modifiers (which aren't treated specially)
    - idk how to handle different keyboard layout yet, so just assume qwerty for now
- users can declare variables
- we also have "default_mask" and "default_multiplicity" by default, but these these are normal user variables to encourage good practices
- users can also pin boolean variables to the bottom task bar and click them to toggle on/off
    - maybe also non-boolean variables

## commands

- command have a guard, which is a boolean expression
- commands are executed (actually put into the resolution set to be resolved at the end of the pass/frame) on the rising edge of a boolean variable
    - not boolean expression bc that is worse for ui
    - or maybe you allow boolean expressions and you don't need the guard?
    - no that gives weird behavior when changing ~keybind sets but you're still holding a key. it would execute the command due to the rising edge of the keybind set boolean and not the key.

## folders

- folders have a color, which is how we color stuff in the keybind reference
- folders hav a guard, which is a boolean expression that determines whether the pass explores the folder

## command list / resolution set

- twist command takes a grip, mask, multiplicity, and whether to invert the twist direction
- only one twist command can be executed per pass (which is a speedsolving rule)
- also only one macro command can be executed per pass (for simplicity and better UX)
- command to set a variable to a value
- for now, just take the first command in the resolution set, even tho it's legal to do multiple non twist commands
- we may also want to fail if there were multiple commands (or multiple twist commands) in the resolution set, but idk which is better UX

## debugging / keybind reference

- should have some debugging thing that shows what was in the resolution set
- show the value of the variables next to where you declare them
- clicking a key in the keybind reference should toggle whether that key is down
- when hovering a key in the keybind reference (or any boolean declaration ig), show inline in the folder tree what would happen if that variable was toggled? this seems kinda hard
