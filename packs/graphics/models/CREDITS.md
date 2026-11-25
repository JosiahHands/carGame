# Car model credits

`cars.glb` holds the six real car bodies that the Photo Real pack puts on the
game's cars. All six are by **Daniel Zhabotinsky**
([sketchfab.com/DanielZhabotinsky](https://sketchfab.com/DanielZhabotinsky)) and
are shared under the **Creative Commons Attribution 4.0** licence
([CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)). Thank you, Daniel!

| Game body | Model | Source |
| --- | --- | --- |
| Sports | '07 Generic Coupe | [sketchfab.com/3d-models/22abe5284d4c4b55920b8462eb24a8c1](https://sketchfab.com/3d-models/07-generic-coupe-low-poly-model-22abe5284d4c4b55920b8462eb24a8c1) |
| Muscle | American Muscle '71 | [sketchfab.com/3d-models/955edc733c6d44fabc0ad7c246a15896](https://sketchfab.com/3d-models/american-muscle-71-low-poly-model-955edc733c6d44fabc0ad7c246a15896) |
| Hatch | Modern Hatchback | [sketchfab.com/3d-models/055ff8a21b8d4d279debca089e2fafcd](https://sketchfab.com/3d-models/modern-hatchback-low-poly-model-055ff8a21b8d4d279debca089e2fafcd) |
| Pickup | Generic 90s light pickup | [sketchfab.com/3d-models/6c48c94be8974dc7a5932f697c0ba68f](https://sketchfab.com/3d-models/generic-90s-light-pickup-low-poly-model-6c48c94be8974dc7a5932f697c0ba68f) |
| Van | Old Chevy van (a generic panel van) | [sketchfab.com/3d-models/09d718c9cf72401b8534d265a06a803f](https://sketchfab.com/3d-models/old-chevy-van-low-poly-model-09d718c9cf72401b8534d265a06a803f) |
| Supercar (wedge) | Prototipo Speciale '95 | [sketchfab.com/3d-models/3ab977050c334c13b6abb5de078b4957](https://sketchfab.com/3d-models/prototipo-speciale-95-low-poly-model-3ab977050c334c13b6abb5de078b4957) |

The files were downloaded from the public copies in the
[Objaverse](https://huggingface.co/datasets/allenai/objaverse) dataset (each file
carries its author, source and licence inside it), and the licence of each was
checked against its Sketchfab page.

## Changes made

- All badges, name plates and number plates were removed (the game draws its own
  plates), and a small emblem printed in the van's interior was painted over.
- The wheels, brakes, spoiler, roll cage and bull bar were removed: the game
  draws these itself from the garage settings.
- Parts hidden under the bonnet (engine) were removed.
- Each body was turned to face forward, centred between its axles and scaled to metres.
- The paint, windows and lamps were given standard names so the game can
  recolour the paint, tint the windows and light the lamps. The hatchback's
  painted panels were split out of its texture for the same reason.
- A simpler copy of each body was added for cars far away.
- Textures were resized to 1024 pixels and saved as WebP; the geometry is
  compressed with meshopt. All six share one file, so shared textures are stored once.

These changes are shared under the same CC BY 4.0 licence.

The designs are generic or loosely based on real cars from their eras; they
are not official models of any car maker and carry no brand names or logos.
