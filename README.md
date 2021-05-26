## MOUSE - Minimal Output User Signal Encoder

# _**Intended features**_
-----------
- Button functionality built into the scroll wheel - One should be able to click the scroll wheel.
- additional buttons found on the mouse: 
1. Mouse sensitivity buttons - one to increase sensitivity and one to reduce it.
2. forward/back buttons - two macro buttons for the user to bind.

- Led user settings.
- Custom shell. 3D printed.

# _**Design overview**_
-----------
![Overview diagram](Pictures/Gaming_Mouse.jpg)

_Buttons provide:_
1. Right and left click.
2. Mouse wheel click.
3. Forward and backwards, which are two macro buttons.
4. Sense up and sense down, two buttons to control mouse sensitivity.

_Sensor provide position data._

_PSU provide:_
1. 3.3V to the buttons and mcu.
2. 1.9V to the sensor.

# _**Circuit design**_
-----------
_**KiCAD diagram**_
-----------
![KiCAD diagram](Pictures/board_schematic.png)

# _**Board design**_
_**KiCAD model**_
-----------
![KiCAD model](Pictures/board_footprints.png)

_**3D model**_
-----------
![3D model diagram](Pictures/3d_board.png)

_**Real board**_
-----------
##### Version 1

![Real board picture](Pictures/Real_board.jpg)
##### Version 2

![Real board v2 picture](Pictures/Real_board_v2.jpg)

# _**Custom case**_
-----------
The case was designed from scratch using Fusion 360.

![mouse design](3d_case/images/mouse_design.png)
![mouse design opacity](3d_case/images/mouse_design_opacity_60.png)
![mouse design board](3d_case/images/mouse_design_board.png)
![mouse design bottom_plate](3d_case/images/mouse_design_bottom_plate.png)
![mouse design sensor cutout](3d_case/images/mouse_design_sensor_cutout.png)

# _**Demo**_
-----------
##### Demo of mouse functionality
[![functionality demo](https://img.youtube.com/vi/CS7f9UfwIrw/0.jpg)](https://www.youtube.com/watch?v=CS7f9UfwIrw "Demo of mouse functionality")

##### Demo of 3D printed case
[![3d case demo](https://img.youtube.com/vi/qKGdOuxL4AI/0.jpg)](https://www.youtube.com/watch?v=qKGdOuxL4AI "Demo of 3d printed case")


# **_Contributors_**
-----------

Edward Källstedt - edwkll-7@student.ltu.se (Grade goal 5)

Kalle Löfgren - kallfg-3@student.ltu.se (Grade goal 5)

Carmen Acín Rouco - caracn-0@student.ltu.se (Grade goal 5)
