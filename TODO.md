# TODO

## uncategorized

- puzzle
- fudging
- keybinds
- macros
- saved preferences
- partial solve detection
- timer
- sidebar
- log files
- fmc timeline
    - operations twist, macro, fork, niss, mark/comment
    - fork is how you search different paths
- thing to derive a subpuzzle by deleting pieces
- refl puzzles
- drag to rotate should be invariant to window size
- BUG: doing several twists accumulates error and makes it block incorrectly. use approx_collections
- blocked red flash should also flash the plane that contains the blocked pieces.
- gizmos
    - should have default shrink <1
    - make "show gizmos" mode look better. outlines? better colors?
    - maybe gizmos should be at the cut depth, not at the faces.
- selected piece sets
- if you do too many blocked moves in a row, offer to switch to an easier puzzle, maybe sun cube ultimate.
- stickerings: super, center compass, edge vs corner triangle distinguishable.
- latched, bandaging pieces, gear
- custom scrambler
- chromebook support??
- finish animation is the cube exploding/popping
- the meet of two blocks is their intersection, the join of two blocks is the smallest block that contains both of them, two blocks are mergeable if their join equals their disjoint union
- probably Action and Move shouldn't both exist
- remove outline width from view settings

## commands / components

- undoable commands?
- what commands should go in the log file?
    - a design goal is that you should be able to nearly reconstruct a screen recording from the log file (tho some mouse movement stuff maybe should be dropped)

### command list

- undo
- redo
- input event (mouse and keyboard)
- mouse drag
- twist
    - maybe keybinds send a anim twist command
    - and once the animation finishes, AnimState sends a puzzle state twist command
- rotate
- realign
- scramble
- reset
- filters
    - next/prev
    - set exactly
    - how to handle sequences?
    - maybe clicking on the sequence will show everything with the sequence fallback, and is its own special stage
- macro

### components list

- puzzle view
    - factor out animations vs geometry?
- puzzle state
- TODO: what is puzzle controller (look at HSC1/2)
- input state
- filters
- timer
- puzzle builder
- scrambler?
- macros
- move input?
- colors?
- stickering?
- replay file viewer?
- fmc timeline?

## filters

- drag and drop reordering
- where should the add buttons be?
- show number of matched pieces, with short circuiting
- example cfop filters
- rename PieceSetTerm to Block (kinda wrong if they include orbits ig)
    - ask on discord about what a piece type is vs orbit
- maybe: factor out filter state (selected index)

## styles

- use styles for hovered / selected / blocked.
- have draw order as part of styles?
- basic's labels should match the partial styles'
- outline_opacity should just be the alpha of the outline color.
- outlines should scale with the size of the puzzle on the screen.

## rendering

- FEAT: lighting
- BUG: outlines are in units of pixels, so resizing the window and scaling the puzzle appears to change their thickness relative to the puzzle.
- FEAT: gizmo outlines?

## keybinds / mousebinds / input

- note mousebinds are part of keybinds,
- keybinds have access to optional hovered_grip_gizmo
mouse and key events are commands,
- keyboard layouts other than qwerty
- mac/not mac cmd/ctrl
- show inline in the folder tree what would happen if a variable were
toggled
- i kinda want default_mask to be derived from 3 boolean flags, which then select 3 masks which are then ored together
- the physical key presses and keybind ref click toggle and status bar variable set should be `Command`s and go in the log file
- why do we need a separate `overrides`? can't we just toggle whether it's in held set?
- make the sidebar a horizontal scroll area as well
