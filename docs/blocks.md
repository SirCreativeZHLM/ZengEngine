# Block reference

Positions are in metres. **x** is left and right, **y** is up, **z** is toward you. "Forward" means the way the object is facing (an object starts facing -z, away from the default camera).

## Events (start a script)
| Block | When it runs |
|---|---|
| when the game starts | once at the beginning |
| when [key] key is pressed / let go | each press or release |
| when I touch [object / anything] | the moment two objects begin to overlap |
| when I am clicked | when you click it with the mouse |
| when I get the message [name] | another script sent that message |
| every [n] seconds | repeatedly on a timer |
| when I start as a copy | on each new copy made by "create a copy" |

## Control
wait, forever, repeat, if, if/else, wait until, repeat until, send message, create a copy of [object / myself], delete me, stop the game. Loops pause for one frame each time round, so a forever loop never freezes the game. A loop with no wait that never pauses shows a problem message.

## Motion
move [direction] by, turn, change/set [x y z], go to, glide, look at [object], set size to %, push by (adds speed; needs physics), set speed (needs physics).

## Looks
set colour, show/hide, say [text] for [n] seconds (0 = stay), show [label] : [value] on screen (a score), set light brightness, camera follows [object], set sky colour.

## Sound
play sound [file]. Import .wav, .mp3, .ogg or .flac files with **+ Import**.

## Physics
- make me [fall and bump / solid wall / moved only by blocks / not physical]
- set gravity, set bounciness.
Only objects with a shape can have physics. "Touching" and "when I touch" work without physics.

## Sensing
key pressed?, touching?, distance to, my x/y/z, x/y/z of [object], seconds since start, mouse position.

## Operators, Variables, Lists, Functions
Maths, comparisons, and/or/not, random, join text, rounding. Variables are shared by every object. Lists hold many values. Functions (Functions category) take inputs and can return a value.
