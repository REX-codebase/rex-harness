// The harness: four nested, precision-machined gyroscopic rings. Each is one of
// the four hard limits a REX run has: steps, tool calls, tokens, time. A ring is
// a bevelled metal body, an engraved tick/label band that turns, and an LED
// arc that shows how much of that budget the run has used.

import {
  Group, Mesh, LatheGeometry, Vector2, MeshPhysicalMaterial, CylinderGeometry, CanvasTexture, ShaderMaterial,
  AdditiveBlending, DoubleSide, SRGBColorSpace, InstancedMesh, Matrix4, Color, Quaternion, Vector3, Euler,
  LinearMipmapLinearFilter, LinearFilter, DynamicDrawUsage, PointLight, DirectionalLight,
} from 'three';
import { U, TAU, clamp, damp } from './state.js';

export const LIMITS = [
  { id: 'steps', label: 'STEPS', flag: '--max-steps', R: 2.55, w: 0.34, color: '#a99af7', tilt: [0.30, 0.0, 0.14], spin: 0.10 },
  { id: 'tools', label: 'TOOL CALLS', flag: '--max-tool-calls', R: 3.3, w: 0.30, color: '#6aa8ff', tilt: [1.12, 0.0, 0.32], spin: -0.075 },
  { id: 'tokens', label: 'TOKENS', flag: '--max-tokens', R: 4.1, w: 0.27, color: '#d9a6ff', tilt: [0.52, 0.0, -0.95], spin: 0.06 },
  { id: 'time', label: 'TIME', flag: '--timeout-secs', R: 5.0, w: 0.24, color: '#ffd7a0', tilt: [0.92, 0.0, 0.82], spin: -0.045 },
];

function profile(R, t, w, cr, n = 5) {
  const pts = [];
  const hx = t / 2, hy = w / 2;
  const corner = (cx, cy, a0) => {
    for (let i = 0; i <= n; i++) {
      const a = a0 + (Math.PI / 2) * (i / n);
      pts.push(new Vector2(R + cx + cr * Math.cos(a), cy + cr * Math.sin(a)));
    }
  };
  corner(hx - cr, -hy + cr, -Math.PI / 2);
  corner(hx - cr, hy - cr, 0);
  corner(-hx + cr, hy - cr, Math.PI / 2);
  corner(-hx + cr, -hy + cr, Math.PI);
  pts.push(pts[0].clone());
  return pts;
}

// Engraved instrument band: ticks every 1.5 degrees, longer ones every 7.5 and
// 15, and the limit's name set around the circumference.
export function bandTexture(label, flag) {
  const W = 4096, H = 128;
  const c = document.createElement('canvas');
  c.width = W;
  c.height = H;
  const g = c.getContext('2d');
  g.clearRect(0, 0, W, H);
  g.fillStyle = '#fff';
  g.strokeStyle = '#fff';
  const ticks = 240;
  for (let i = 0; i < ticks; i++) {
    const x = (i / ticks) * W;
    const big = i % 10 === 0, mid = i % 5 === 0;
    const len = big ? 40 : mid ? 26 : 14;
    g.globalAlpha = big ? 1 : mid ? 0.8 : 0.55;
    g.fillRect(x - (big ? 2 : 1), 4, big ? 4 : 2, len);
  }
  g.globalAlpha = 1;
  const text = `${label}  ·  HARD LIMIT  ·  ${flag}  ·  `;
  g.font = '600 30px "REX Display", Orbitron, sans-serif';
  g.textBaseline = 'middle';
  g.fillStyle = '#fff';
  const reps = 4;
  const seg = W / reps;
  for (let r = 0; r < reps; r++) {
    g.save();
    g.translate(r * seg + 40, 84);
    g.scale(0.82, 1);
    g.fillText(text.toUpperCase(), 0, 0);
    g.restore();
  }
  // numerals at the 15-degree marks
  g.font = '500 20px "REX Mono", monospace';
  g.globalAlpha = 0.85;
  for (let i = 0; i < 24; i++) {
    g.fillText(String(i * 15).padStart(3, '0'), (i / 24) * W + 8, 56);
  }
  const tex = new CanvasTexture(c);
  tex.colorSpace = SRGBColorSpace;
  tex.anisotropy = 8;
  tex.minFilter = LinearMipmapLinearFilter;
  tex.magFilter = LinearFilter;
  tex.generateMipmaps = true;
  return tex;
}

const BAND_FRAG = /* glsl */ `
uniform sampler2D tMap; uniform vec3 uColor; uniform float uTier, uGlow, uAlive;
varying vec2 vUv;
void main(){
  vec4 t = texture2D(tMap, vec2(vUv.x * 1., vUv.y));
  float a = t.a;
  vec3 warm = mix(uColor, uColor * vec3(1.25, .9, .6), uTier);
  vec3 col = warm * (1.05 + 1.6 * uGlow) * a;
  float edge = smoothstep(0., .12, vUv.y) * smoothstep(1., .88, vUv.y);
  gl_FragColor = vec4(col * edge * uAlive, a * edge);
}`;

const ARC_FRAG = /* glsl */ `
uniform vec3 uColor; uniform float uFill, uTime, uTier, uAlive, uHalt;
varying vec2 vUv;
void main(){
  float u = vUv.x;
  float on = step(u, uFill);
  float seg = .5 + .5 * step(.35, fract(u * 160.));
  float head = exp(-abs(u - uFill) * 70.) * step(.002, uFill);
  vec3 c = mix(uColor, uColor * vec3(1.25, .9, .6), uTier);
  c = mix(c, vec3(1., .55, .15), uHalt * step(.985, uFill));
  vec3 col = c * (.16 + on * seg * 2.3) + c * head * 5.5;
  float v = smoothstep(0., .2, vUv.y) * smoothstep(1., .8, vUv.y);
  gl_FragColor = vec4(col * v * uAlive, 1.);
}`;

const BAND_VERT = 'varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }';

export function createRings({ quality, envMap }) {
  const root = new Group();
  root.name = 'harness';
  const rings = [];
  const segs = quality.segs;

  const metal = new MeshPhysicalMaterial({
    color: new Color(0x34314a), metalness: 1, roughness: 0.24, clearcoat: 0.6, clearcoatRoughness: 0.14,
    envMap, envMapIntensity: 2.4,
  });
  const studMat = new MeshPhysicalMaterial({ color: 0x23202e, metalness: 1, roughness: 0.22, envMap, envMapIntensity: 1.4 });

  const light = new PointLight(0xb9a8ff, 120, 0, 2);
  root.add(light);
  const key = new DirectionalLight(0xe9e2ff, 2.2);
  key.position.set(-6, 8, 7);
  root.add(key);
  const rim = new DirectionalLight(0x6f8dff, 1.6);
  rim.position.set(7, 2, -8);
  root.add(rim);

  LIMITS.forEach((L, idx) => {
    const g = new Group();
    g.rotation.set(L.tilt[0], L.tilt[1], L.tilt[2]);
    const t = 0.13;
    const body = new Mesh(new LatheGeometry(profile(L.R, t, L.w, 0.04), segs), metal);
    g.add(body);

    const spin = new Group();
    g.add(spin);

    const tex = bandTexture(L.label, L.flag);
    const bandMat = new ShaderMaterial({
      uniforms: { tMap: { value: tex }, uColor: { value: new Color(L.color) }, uTier: U.uTier, uGlow: { value: 0.0 }, uAlive: U.uAlive },
      vertexShader: BAND_VERT,
      fragmentShader: BAND_FRAG,
      transparent: true,
      blending: AdditiveBlending,
      depthWrite: false,
      side: DoubleSide,
    });
    const band = new Mesh(new CylinderGeometry(L.R + t / 2 + 0.003, L.R + t / 2 + 0.003, L.w * 0.7, segs, 1, true), bandMat);
    band.position.y = L.w * 0.02;
    spin.add(band);

    const arcMat = new ShaderMaterial({
      uniforms: { uColor: { value: new Color(L.color) }, uFill: { value: 0.4 }, uTime: U.uTime, uTier: U.uTier, uAlive: U.uAlive, uHalt: { value: 0 } },
      vertexShader: BAND_VERT,
      fragmentShader: ARC_FRAG,
      transparent: true,
      blending: AdditiveBlending,
      depthWrite: false,
      side: DoubleSide,
    });
    const arc = new Mesh(new CylinderGeometry(L.R + t / 2 + 0.006, L.R + t / 2 + 0.006, 0.075, segs, 1, true), arcMat);
    arc.position.y = -L.w / 2 + 0.06;
    g.add(arc);

    // studs: six radial bolts per ring
    const studs = new InstancedMesh(new CylinderGeometry(0.034, 0.034, 0.1, 12), studMat, 6);
    const m = new Matrix4(), q = new Quaternion(), e = new Euler(), p = new Vector3(), s = new Vector3(1, 1, 1);
    for (let i = 0; i < 6; i++) {
      const a = (i / 6) * TAU + idx * 0.4;
      p.set(Math.cos(a) * (L.R + t / 2 + 0.01), 0, Math.sin(a) * (L.R + t / 2 + 0.01));
      e.set(0, -a, Math.PI / 2);
      q.setFromEuler(e);
      m.compose(p, q, s);
      studs.setMatrixAt(i, m);
    }
    g.add(studs);

    root.add(g);
    rings.push({ L, group: g, spin, bandMat, arcMat, fill: 0.4, glow: 0 });
  });

  return {
    object: root,
    rings,
    light,
    // normals of each ring plane (world), used by the particle belts
    update(dt, t, st) {
      const tier = U.uTier.value;
      light.color.setRGB(0.72 + 0.28 * tier, 0.66 - 0.04 * tier, 1.0 - 0.55 * tier);
      for (const r of rings) {
        r.spin.rotation.y += r.L.spin * dt;
        const target = st.fill[r.L.id] ?? 0.4;
        r.fill = damp(r.fill, target, 6, dt);
        r.arcMat.uniforms.uFill.value = r.fill;
        r.arcMat.uniforms.uHalt.value = st.halted ? 1 : 0;
        r.glow = damp(r.glow, st.glow[r.L.id] ?? 0, 5, dt);
        r.bandMat.uniforms.uGlow.value = r.glow;
      }
    },
  };
}
