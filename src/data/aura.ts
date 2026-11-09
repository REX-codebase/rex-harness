export type Box = { left: number; top: number; width: number; height: number };

// Where the Ultra shockwave starts: the centre of the control that was
// activated. A keyboard "click" reports 0,0 for its pointer, so the control's
// own box is the only honest origin.
//
// (Pointer-following light, field spotlights and the cursor halo belong to the
// motion layer: see Fx.tsx / motion-ui.css.)
export function originOf(box: Box): { x: number; y: number } {
  return { x: Math.round(box.left + box.width / 2), y: Math.round(box.top + box.height / 2) };
}
