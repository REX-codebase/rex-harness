// Quality tiers and the governor that moves between them.
//
// The page never assumes a GPU. It guesses a starting tier from the adapter and
// device, then watches real frame times and sheds resolution first (cheap and
// almost invisible), particles second. `?q=high|medium|low` pins a tier and
// switches the governor off, for benchmarking.

export const TIERS = {
  high: { name: 'high', dprMax: 2, msaa: 4, stars: 16000, particles: 200000, dust: 1400, bloomLevels: 6, nebula: 5, segs: 256 },
  medium: { name: 'medium', dprMax: 1.5, msaa: 0, stars: 10000, particles: 100000, dust: 900, bloomLevels: 5, nebula: 4, segs: 192 },
  low: { name: 'low', dprMax: 1, msaa: 0, stars: 5000, particles: 36000, dust: 500, bloomLevels: 4, nebula: 3, segs: 128 },
};
const ORDER = ['low', 'medium', 'high'];

export function adapterInfo(gl) {
  const ext = gl.getExtension('WEBGL_debug_renderer_info');
  const renderer = ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
  return { renderer: String(renderer), software: /swiftshader|llvmpipe|software|basic render/i.test(String(renderer)) };
}

export function pickTier(adapter, params) {
  if (params.q && TIERS[params.q]) return { tier: TIERS[params.q], pinned: true };
  const mobile = matchMedia('(pointer: coarse)').matches || /Android|iPhone|iPad/i.test(navigator.userAgent);
  const cores = navigator.hardwareConcurrency || 4;
  const mem = navigator.deviceMemory || 4;
  if (adapter.software) return { tier: TIERS.low, pinned: false };
  if (navigator.connection && navigator.connection.saveData) return { tier: TIERS.low, pinned: false };
  if (mobile) return { tier: cores >= 8 && mem >= 6 ? TIERS.medium : TIERS.low, pinned: false };
  if (cores <= 4 || mem <= 4) return { tier: TIERS.medium, pinned: false };
  return { tier: TIERS.high, pinned: false };
}

export class Governor {
  constructor(tier, pinned, onChange) {
    this.tier = tier;
    this.pinned = pinned;
    this.onChange = onChange;
    this.scale = 1; // dynamic resolution multiplier on the tier's dprMax
    this.acc = 0;
    this.n = 0;
    this.warm = 90; // ignore shader compilation and first-paint frames
    this.fps = 60;
    this.ups = 0; // how many times we tried going back up
    this.calm = 0;
  }

  // dt in milliseconds between frames
  sample(dt) {
    if (dt > 250) return; // tab was hidden, or a compile hitch
    if (this.warm > 0) {
      this.warm--;
      return;
    }
    this.acc += dt;
    this.n++;
    if (this.n < 45) return;
    const avg = this.acc / this.n;
    this.fps = 1000 / avg;
    this.acc = 0;
    this.n = 0;
    if (this.pinned) return;
    if (avg > 21) {
      this.calm = 0;
      if (this.scale > 0.66) this.scale = Math.max(0.6, this.scale - 0.12);
      else this.dropTier();
      this.onChange(this);
    } else if (avg < 17.6 && this.scale < 1 && this.ups < 2) {
      if (++this.calm >= 6) {
        this.calm = 0;
        this.ups++;
        this.scale = Math.min(1, this.scale + 0.12);
        this.onChange(this);
      }
    }
  }

  dropTier() {
    const i = ORDER.indexOf(this.tier.name);
    if (i > 0) {
      this.tier = TIERS[ORDER[i - 1]];
      this.scale = 0.85;
    }
  }
}
