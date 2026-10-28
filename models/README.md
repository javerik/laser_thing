# Test models

## `l-block.stl`

A stepped, asymmetric L for checking a metric 3D scan against its model (RACS's *Check against
STL* and measuring tool, rustcamserve #33). Watertight ASCII STL in mm, z up, resting on z = 0:

- a **slab**, 80 × 15 × 6 mm: x 0…80, y 0…15
- an **upright**, 20 × 60 × 20 mm: x 0…20, y 0…60, at the slab's low-x end

With the default slice settings (+Z up, travel along Y, so u = x, v = y, not mirrored), laserline
centres it in u and reports `object_mm` min (−40, 0, 6), max (40, 60, 20).

What to measure on the scan:

| | STL |
|---|---|
| length along the line (Δu) | 80 mm |
| length along the travel (Δv) | 60 mm |
| top height (z) | 20 mm |
| slab height above the base | 6 mm |
| step from the slab to the upright | 14 mm |
| upright width, along the line | 20 mm |
| slab width, along the travel | 15 mm |

Handedness: seen from above with the travel pointing away, the upright runs along the left edge
(low u) and the slab along the near edge (low v). A mirrored scan shows the upright on the right.
