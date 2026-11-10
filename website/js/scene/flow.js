// The agent's activity, made visible: up to 200k GPU particles. Most hug the
// harness rings in thin counter-rotating belts, some form a diffuse accretion
// disc, a few stray into a halo. All motion is computed in the vertex shader from
// per-particle attributes, so the CPU does no per-particle work at all.

import { Points, BufferGeometry, BufferAttribute, ShaderMaterial, AdditiveBlending, Euler, Vector3, Quaternion } from 'three';
import { U, TAU } from './state.js';
import { LIMITS } from './rings.js';

const VERT = /* glsl */ `
attribute vec3 aBase; attribute vec3 aAxis; attribute vec4 aOrb;
uniform float uFlowT, uPR, uTier, uFreeze, uAlive, uTime;
varying vec3 vCol;
vec3 rot(vec3 p, vec3 k, float a){ float c = cos(a), s = sin(a); return p * c + cross(k, p) * s + k * dot(k, p) * (1. - c); }
void main(){
  float ang = aOrb.x * uFlowT;
  vec3 p = rot(aBase, aAxis, ang);
  p += aAxis * sin(uFlowT * .55 + aOrb.w * 40.) * .035;
  vec4 mv = modelViewMatrix * vec4(p, 1.);
  float dist = max(-mv.z, .5);
  float r = length(aBase);
  gl_PointSize = clamp(aOrb.y * uPR * (34. / dist), 2.2 * uPR, 11. * uPR);
  gl_Position = projectionMatrix * mv;

  float heat = smoothstep(7.2, 1.5, r);
  vec3 cool = mix(vec3(.16, .10, .72), vec3(.50, .26, .14), uTier);
  vec3 mid = mix(vec3(.52, .40, 1.00), vec3(1.00, .68, .32), uTier);
  vec3 hot = mix(vec3(.95, .92, 1.00), vec3(1.00, .95, .82), uTier);
  vec3 col = mix(cool, mid, smoothstep(0., .55, heat));
  col = mix(col, hot, smoothstep(.55, 1., heat));
  float tw = .75 + .25 * sin(uTime * (1.2 + aOrb.w * 2.) + aOrb.w * 99.);
  col *= aOrb.z * tw * .36;
  col = mix(col, vec3(1., .6, .2) * dot(col, vec3(.33)) * 1.1, uFreeze * .85);
  vCol = col * uAlive;
}`;

const FRAG = /* glsl */ `
varying vec3 vCol;
void main(){
  vec2 p = gl_PointCoord * 2. - 1.;
  float a = exp(-dot(p, p) * 3.4);
  if (a < .01) discard;
  gl_FragColor = vec4(vCol * a, 1.);
}`;

function mulberry(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const gauss = (r) => (r() + r() + r() + r() - 2) * 0.9;

export function createFlow({ quality }) {
  const N = quality.particles;
  const rnd = mulberry(0x7e57);
  const base = new Float32Array(N * 3);
  const axis = new Float32Array(N * 3);
  const orb = new Float32Array(N * 4);

  // ring-plane normals, matching the tilts the rings use
  const normals = LIMITS.map((L) => new Vector3(0, 1, 0).applyQuaternion(new Quaternion().setFromEuler(new Euler(...L.tilt))));
  const basisU = (n) => {
    const a = Math.abs(n.y) < 0.9 ? new Vector3(0, 1, 0) : new Vector3(1, 0, 0);
    const u = new Vector3().crossVectors(a, n).normalize();
    return [u, new Vector3().crossVectors(n, u)];
  };
  const bases = normals.map(basisU);

  const put = (i, pos, ax, speed, size, bright, seed) => {
    base.set([pos.x, pos.y, pos.z], i * 3);
    axis.set([ax.x, ax.y, ax.z], i * 3);
    orb.set([speed, size, bright, seed], i * 4);
  };
  const tmp = new Vector3();

  for (let i = 0; i < N; i++) {
    const k = rnd();
    if (k < 0.58) {
      // belts hugging a ring
      const ri = Math.floor(rnd() * 4);
      const L = LIMITS[ri];
      const n = normals[ri], [u, v] = bases[ri];
      const rr = L.R + gauss(rnd) * (0.16 + 0.03 * L.R);
      const a = rnd() * TAU;
      const h = gauss(rnd) * (0.05 + 0.015 * L.R);
      tmp.copy(u).multiplyScalar(Math.cos(a) * rr).addScaledVector(v, Math.sin(a) * rr).addScaledVector(n, h);
      const dir = ri % 2 ? -1 : 1;
      put(i, tmp, n, dir * 0.95 * Math.pow(L.R, -1.5) * 2.2, 0.8 + rnd() * 1.5, 0.35 + rnd() * 0.75, rnd());
    } else if (k < 0.88) {
      // diffuse accretion disc around the main plane with spiral arms
      const rr = 1.55 + Math.pow(rnd(), 1.6) * 7.4;
      const arm = Math.floor(rnd() * 3);
      const a = (arm / 3) * TAU + 1.5 * Math.log(rr) + gauss(rnd) * 0.55;
      const tilt = new Euler(0.18, 0, -0.12);
      const n = new Vector3(0, 1, 0).applyEuler(tilt);
      const [u, v] = basisU(n);
      const h = gauss(rnd) * (0.04 + 0.05 * rr);
      tmp.copy(u).multiplyScalar(Math.cos(a) * rr).addScaledVector(v, Math.sin(a) * rr).addScaledVector(n, h);
      put(i, tmp, n, 0.8 * Math.pow(rr, -1.5) * 2.2, 0.7 + rnd() * 1.1, 0.18 + rnd() * 0.55, rnd());
    } else {
      // halo: random orientation, slow
      const rr = 2.2 + Math.pow(rnd(), 1.3) * 9;
      const z = rnd() * 2 - 1, a = rnd() * TAU, s = Math.sqrt(1 - z * z);
      tmp.set(s * Math.cos(a) * rr, z * rr * 0.8, s * Math.sin(a) * rr);
      const ax = new Vector3(rnd() - 0.5, rnd() - 0.2, rnd() - 0.5).normalize();
      put(i, tmp, ax, (rnd() < 0.5 ? -1 : 1) * 0.12 * Math.pow(rr, -0.5), 0.7 + rnd() * 1.0, 0.10 + rnd() * 0.30, rnd());
    }
  }

  const g = new BufferGeometry();
  g.setAttribute('position', new BufferAttribute(base, 3)); // required by three for bounds; real motion is in the shader
  g.setAttribute('aBase', new BufferAttribute(base, 3));
  g.setAttribute('aAxis', new BufferAttribute(axis, 3));
  g.setAttribute('aOrb', new BufferAttribute(orb, 4));
  const mat = new ShaderMaterial({
    uniforms: { uFlowT: U.uFlowT, uPR: U.uPR, uTier: U.uTier, uFreeze: U.uFreeze, uAlive: U.uAlive, uTime: U.uTime },
    vertexShader: VERT,
    fragmentShader: FRAG,
    transparent: true,
    blending: AdditiveBlending,
    depthWrite: false,
  });
  const points = new Points(g, mat);
  points.frustumCulled = false;
  points.name = 'flow';
  g.setDrawRange(0, N);
  return { object: points, update() {} };
}
