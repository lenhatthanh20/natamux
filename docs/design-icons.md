# Design: status icons and motion (T-0058)

The state glyphs, the stage markers and every moving icon of the dashboard, redrawn as one family. Two rules: smooth and good-looking, and no extra CPU, GPU or memory. The rules for building with them are in `ui-spec.md` 2.7, 2.8 and 4.10.

## Why

- The app draws endless CSS animations at 8 frames a second (`lib/motion-ticker.ts`, one tick every 125 ms). The old 900 ms spinner moved about 50° per tick and landed on a new angle every turn, so it looked choppy.
- "Running" looked different in three places: a spinning arc in the table, a breathing dot in the sidebar, an arc with a halo on stage markers.
- Reduced motion had two rules: the busy button froze a 3/4 arc, `StatusGlyph` swapped to `CircleEllipsis` through a `matchMedia` listener per glyph.
- The skeleton animated `background-position`, which repaints every bar on every frame.
- The sidebar glyph was 8 px: below the cap height of 13 px Inter, so it read as a bullet, and the small shapes blurred.

## Before

| Where | What | Motion |
|---|---|---|
| Table, palette, pane tabs, terminal header | `StatusGlyph`: lucide circle icons, 14 px, stroke 2.25 | Running: `LoaderCircle` rotates 900 ms |
| Busy buttons (about 30 call sites) | `Button` `busy`: `LoaderCircle` | Rotates 900 ms |
| Sidebar rows | `ShapeGlyph`: own shapes, 8 px (9 on attention rows), shells 7 px | Running disk: opacity 2.4 s |
| Stage markers | `StageMarker`: conic arc with a mask, halo, core | Arc 900 ms, halo 1.8 s, review core 2.4 s |
| Link into a running stage | Stripes | `translateX` 900 ms |
| Loading | `Skeleton`: shimmer gradient | `background-position` 1.5 s |
| Action icons | lucide, stroke 1.75 units (1.0 px at 14, 2.3 px at 32) | — |

## Designs compared

Three designs were made independently from the same brief, with a live preview at the app's 8 fps.

| | 1 · Precision line | 2 · Solid signal | 3 · Living progress |
|---|---|---|---|
| Idea | A ring and one mark, one pen, close to lucide | One filled silhouette per state | One circle that fills up through the task's life |
| At 8 px | Rings look alike | Every shape differs | 1/4, 1/2, 3/4 look alike |
| Running | Sweep hand, 15° per tick | Disk blinks once per 2 s | Quarter jumps 90° per 500 ms |
| Busy spinner | Arc on a track | Four squares | 8 dashes, one per tick |
| Weak point | Small sizes | Bare check and dash | The fill suggests progress that is not real |

## Picked

Design 2 for the shapes, the running blink and the stage markers; the spinner of design 3. Design 2 is the only set that reads at sidebar size and differs by shape at every size, and it grows out of the sidebar shapes the app had. The blink is the calmest with many tasks running: 3 of 16 frames change.

| State | Shape |
|---|---|
| draft | dashed ring |
| ready | ring |
| running | full disk, blinks |
| running but quiet | ring, lower half filled, needs-input color |
| needs-input | diamond |
| review | rounded square |
| blocked | two bars |
| failed | triangle |
| done | check |
| cancelled | dash |
| state not known yet | ring with a dot, muted |

| Item | Property | Duration | Frames that differ at 8 fps | Still form |
|---|---|---|---|---|
| Running glyph and stage marker (`.hm-beat`) | `opacity`, `scale` on the outer svg | 2000 ms | 3 of 16 | the disk |
| Busy spinner (`.hm-spin8`) | `rotate`, `steps(8)` | 1000 ms | 8 of 8 | three dots |
| Link into a running stage | `scaleX`, then `opacity`, on `::before` | 2000 ms | 9 of 16 | a full line |
| Skeleton list (`.hm-pulse`) | `opacity` on the list | 2000 ms | 16 | still bars |

- Sizes: the sidebar task glyph is 10 px for every row tier, shell rows 8 px; the 14 px slot did not change, so rows do not move. The table glyph is the same 10 px drawing in a 16 px box.
- A window without focus stops every endless animation, as before.
- Action icons stay lucide. Their stroke is set in screen pixels: 1.25 px at 12 to 16 px, 1.5 px at 20 and 32 px.

## Cost

Estimated from the property each item animates; not profiled.

| Item | Before | After |
|---|---|---|
| Running glyph | 1 moving node + 1 `matchMedia` listener | 1 moving node, no listener |
| Stage marker | 2 moving nodes, one with a `mask` | 1 moving node, no mask |
| Review marker | 1 moving node | still |
| Skeleton | every bar repaints on every frame | 1 opacity layer per list, no repaint |
| Endless icon keyframes | spin, breathe, halo, review breathe, flow, shimmer | beat, spin, fill, pulse |
