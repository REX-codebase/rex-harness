// Chapter 01: the agent's proposals travel from the entity to a gate and wait
// for a human; only an approved change reaches the disposable copy.
//
//   entity ──▶ gate (your approval) ──▶ disposable copy (a lattice of "files")
//
// The HTML demo owns the decision. This listens to its events and shows what
// each decision does in space.

import {
  Group, Mesh, TorusGeometry, CircleGeometry, ShaderMaterial, MeshPhysicalMaterial, InstancedMesh, BoxGeometry, InstancedBufferAttribute,
  Matrix4, Quaternion, Vector3, Color, AdditiveBlending, DoubleSide, OctahedronGeometry, MeshBasicMaterial, PlaneGeometry, LineSegments,
  EdgesGeometry, LineBasicMaterial, DynamicDrawUsage,
} from 'three';
import { U, S, TAU, clamp, smooth, damp, lerp } from './state.js';
import { sim, on, emit } from '../core.js';
import { PIPE } from './rig.js';
import { createFX } from './fx.js';

const DIR = new Vector3(...PIPE.dir);
const GATE = DIR.clone().multiplyScalar(PIPE.gate);
const COPY = DIR.clone().multiplyScalar(PIPE.copy);

const PORTAL_VERT = 'varying vec2 vP; void main(){ vP = position.xy; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }';
const PORTAL_FRAG = /* glsl */ `
uniform float uTime, uPulse, uVis; uniform vec3 uColor;
varying vec2 vP;
void main(){
  float r = length(vP) / 2.4;
  float edge = smoothstep(.55, 1., r);
  float rings = .5 + .5 * sin(r * 26. - uTime * 2.2);
  float a = atan(vP.y, vP.x);
  float hex = .5 + .5 * sin(a * 12. + r * 6. + uTime * .6);
  float body = (.006 + .022 * rings * hex) * (1. - edge);
  float sweep = exp(-pow((fract(uTime * .3) - r) * 9., 2.)) * .35;
  float k = body + edge * .7 + sweep * (1. - edge) + uPulse * (1. - r) * 1.1;
  gl_FragColor = vec4(uColor * k * uVis, k * uVis * .9);
}`;

const VOX_VERT = /* glsl */ `
attribute float aT; attribute float aSeed;
uniform float uTime;
varying vec3 vN; varying vec3 vV; varying float vPulse; varying float vMod; varying float vSeed; varying vec2 vUv;
void main(){
  vUv = uv;
  mat4 m = modelMatrix * instanceMatrix;
  vec4 wp = m * vec4(position, 1.);
  vN = normalize(mat3(m) * normal);
  vV = normalize(cameraPosition - wp.xyz);
  float age = uTime - aT;
  vMod = aT > 0. ? 1. : 0.;
  vPulse = aT > 0. ? exp(-age * 1.1) : 0.;
  vSeed = aSeed;
  gl_Position = projectionMatrix * viewMatrix * wp;
}`;
const VOX_FRAG = /* glsl */ `
uniform float uTime, uVis, uTier;
varying vec3 vN; varying vec3 vV; varying float vPulse; varying float vMod; varying float vSeed; varying vec2 vUv;
void main(){
  float fr = pow(1. - abs(dot(normalize(vN), normalize(vV))), 2.2);
  float shimmer = .5 + .5 * sin(uTime * .8 + vSeed * 40.);
  float eg = min(min(vUv.x, 1. - vUv.x), min(vUv.y, 1. - vUv.y));
  float line = 1. - smoothstep(.02, .10, eg);
  vec3 base = vec3(.035, .032, .075) + vec3(.16, .14, .34) * fr * (.6 + .4 * shimmer) + vec3(.34, .28, .70) * line * (.45 + .35 * shimmer);
  vec3 lav = vec3(.55, .45, 1.);
  vec3 mint = vec3(.40, 1., .78);
  vec3 col = base + vMod * lav * (.30 + .5 * fr + .9 * line) + vPulse * mint * (1.4 + 2.2 * line);
  gl_FragColor = vec4(col * uVis, uVis);
}`;

const GLOW_VERT = 'uniform float uSize; varying vec2 vUv; void main(){ vUv = uv * 2. - 1.; vec4 mv = modelViewMatrix * vec4(0., 0., 0., 1.); mv.xy += position.xy * uSize; gl_Position = projectionMatrix * mv; }';
const GLOW_FRAG = 'uniform vec3 uColor; uniform float uVis; varying vec2 vUv; void main(){ float r = length(vUv); float a = exp(-r * 4.2) + exp(-r * 14.) * 1.4; gl_FragColor = vec4(uColor * a * uVis, a * uVis); }';

export function createCustody({ labels }) {
  const group = new Group();
  group.name = 'custody';
  group.visible = false;
  const fx = createFX(700);
  group.add(fx.object);

  // ── gate ──
  const gate = new Group();
  gate.position.copy(GATE);
  gate.quaternion.setFromUnitVectors(new Vector3(0, 0, 1), DIR);
  const metal = new MeshPhysicalMaterial({ color: 0x34314a, metalness: 1, roughness: 0.24, clearcoat: 0.6, envMapIntensity: 2.2 });
  const torus = new Mesh(new TorusGeometry(2.5, 0.085, 20, 128), metal);
  gate.add(torus);
  const portalMat = new ShaderMaterial({
    uniforms: { uTime: U.uTime, uPulse: { value: 0 }, uVis: { value: 0 }, uColor: { value: new Color(0.45, 0.36, 1) } },
    vertexShader: PORTAL_VERT, fragmentShader: PORTAL_FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false, side: DoubleSide,
  });
  gate.add(new Mesh(new CircleGeometry(2.42, 96), portalMat));
  const studs = new InstancedMesh(new BoxGeometry(0.05, 0.2, 0.05), metal, 36);
  {
    const m = new Matrix4(), q = new Quaternion(), p = new Vector3(), s = new Vector3(1, 1, 1);
    for (let i = 0; i < 36; i++) {
      const a = (i / 36) * TAU;
      p.set(Math.cos(a) * 2.5, Math.sin(a) * 2.5, 0);
      q.setFromAxisAngle(new Vector3(0, 0, 1), a);
      m.compose(p, q, s);
      studs.setMatrixAt(i, m);
    }
  }
  gate.add(studs);
  group.add(gate);

  // ── disposable copy: a hollow cube of voxels ──
  const lattice = new Group();
  lattice.position.copy(COPY);
  lattice.quaternion.setFromUnitVectors(new Vector3(0, 0, 1), DIR);
  const N = 7, step = 0.42, vs = 0.31;
  const cells = [];
  for (let x = 0; x < N; x++) for (let y = 0; y < N; y++) for (let z = 0; z < N; z++) {
    if (x > 0 && x < N - 1 && y > 0 && y < N - 1 && z > 0 && z < N - 1) continue;
    cells.push([x, y, z]);
  }
  const voxMat = new ShaderMaterial({
    uniforms: { uTime: U.uTime, uVis: { value: 0 }, uTier: U.uTier },
    vertexShader: VOX_VERT, fragmentShader: VOX_FRAG, transparent: true,
  });
  const vox = new InstancedMesh(new BoxGeometry(1, 1, 1), voxMat, cells.length);
  const aT = new Float32Array(cells.length), aSeed = new Float32Array(cells.length);
  {
    const m = new Matrix4(), q = new Quaternion(), s = new Vector3(vs, vs, vs), p = new Vector3();
    const o = ((N - 1) * step) / 2;
    cells.forEach(([x, y, z], i) => {
      p.set(x * step - o, y * step - o, z * step - o);
      m.compose(p, q, s);
      vox.setMatrixAt(i, m);
      aSeed[i] = Math.random();
    });
  }
  const aTAttr = new InstancedBufferAttribute(aT, 1);
  aTAttr.setUsage(DynamicDrawUsage);
  vox.geometry.setAttribute('aT', aTAttr);
  vox.geometry.setAttribute('aSeed', new InstancedBufferAttribute(aSeed, 1));
  vox.frustumCulled = false;
  lattice.add(vox);
  const cageMat = new LineBasicMaterial({ color: 0x8d7aff, transparent: true, opacity: 0, blending: AdditiveBlending, depthWrite: false });
  const cage = new LineSegments(new EdgesGeometry(new BoxGeometry(3.45, 3.45, 3.45)), cageMat);
  lattice.add(cage);
  group.add(lattice);

  // voxels on the face toward the gate are where approved changes land
  const faceCells = cells.map((c, i) => [c, i]).filter(([c]) => c[2] === 0);
  const half = ((N - 1) * step) / 2;
  const voxelWorld = (i, out) => {
    const [x, y, z] = cells[i];
    out.set(x * step - half, y * step - half, z * step - half).applyQuaternion(lattice.quaternion).add(lattice.position);
    return out;
  };

  // ── packets ──
  const packets = [];
  const mkPacket = (id) => {
    const g = new Group();
    const core = new Mesh(new OctahedronGeometry(0.2, 0), new MeshBasicMaterial({ color: new Color(2.6, 2.2, 3.4), toneMapped: false }));
    const glowMat = new ShaderMaterial({ uniforms: { uColor: { value: new Color(0.55, 0.42, 1) }, uVis: { value: 1 }, uSize: { value: 0.7 } }, vertexShader: GLOW_VERT, fragmentShader: GLOW_FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false });
    const glow = new Mesh(new PlaneGeometry(2, 2), glowMat);
    glow.frustumCulled = false;
    g.add(core, glow);
    group.add(g);
    return { id, g, core, glow: glowMat, state: 'fly', s: 0, from: new Vector3(), target: new Vector3(), landing: 0 };
  };
  const offs = [];
  offs.push(on('propose', (p) => packets.push(mkPacket(p.id))));
  offs.push(on('decision', ({ id, decision }) => {
    const pk = packets.find((x) => x.id === id);
    if (!pk) return;
    if (decision === 'approve') {
      pk.state = 'pass';
      pk.s = 0;
      pk.landing = faceCells[(Math.random() * faceCells.length) | 0][1];
      voxelWorld(pk.landing, pk.target);
      pk.from.copy(pk.g.position);
      gateState.set('approve');
    } else {
      pk.state = 'die';
      fx.burst(pk.g.position, '#ff7a90', 70, 3.4, 1.2, 3.2);
      gateState.set('reject');
      group.remove(pk.g);
      packets.splice(packets.indexOf(pk), 1);
    }
  }));

  // gate colour state machine
  const COL = { idle: new Color(0.40, 0.32, 1.0).multiplyScalar(0.9), pending: new Color(1.0, 0.64, 0.23), approve: new Color(0.45, 1.0, 0.72), reject: new Color(1.0, 0.42, 0.52) };
  const gateState = { cur: 'idle', t: 0, pulse: 0, color: COL.idle.clone(),
    set(s) { this.cur = s; this.t = 0; this.pulse = 1; } };

  // ── labels ──
  const lAgent = labels.add({ text: 'The agent', sub: 'proposes', anchor: new Vector3(0, 3.4, 0), chapter: 'custody', tiny: true });
  const lGate = labels.add({ text: 'Your approval', anchor: GATE.clone().add(new Vector3(0, 3.35, 0)), chapter: 'custody', cls: 'is-gold', tiny: true });
  const lCopy = labels.add({ text: 'Disposable copy', sub: 'of your folder', anchor: COPY.clone().add(new Vector3(0, -2.5, 0)), chapter: 'custody', tiny: true });

  const p0 = new Vector3(), v = new Vector3(), up = new Vector3(0, 1, 0);
  const ease = (x) => x * x * (3 - 2 * x);

  return {
    object: group,
    dispose() { offs.forEach((f) => f()); },
    update(dt) {
      const w = S.w.custody || 0;
      group.visible = w > 0.01;
      if (!group.visible) return;
      portalMat.uniforms.uVis.value = w;
      voxMat.uniforms.uVis.value = w;
      cageMat.opacity = 0.35 * w;
      const dur = S.reduced ? 0.6 : 2.3;

      // gate colour
      gateState.t += dt;
      let target = COL.idle;
      if (packets.some((p) => p.state === 'wait')) target = sim.mode === 'auto' ? COL.idle : COL.pending;
      if (gateState.cur === 'approve' && gateState.t < 1.1) target = COL.approve;
      if (gateState.cur === 'reject' && gateState.t < 1.0) target = COL.reject;
      gateState.color.lerp(target, 1 - Math.exp(-9 * dt));
      gateState.pulse = Math.max(0, gateState.pulse - dt * 1.4);
      portalMat.uniforms.uColor.value.copy(gateState.color);
      portalMat.uniforms.uPulse.value = gateState.pulse + (target === COL.pending ? 0.18 + 0.12 * Math.sin(U.uTime.value * 5) : 0);

      for (let i = packets.length - 1; i >= 0; i--) {
        const pk = packets[i];
        if (pk.state === 'fly') {
          pk.s = Math.min(1, pk.s + dt / dur);
          const e = ease(pk.s);
          p0.copy(DIR).multiplyScalar(1.7 + e * (PIPE.gate - 1.7));
          p0.y += Math.sin(Math.PI * e) * 1.1;
          pk.g.position.copy(p0);
          if (pk.s >= 1) { pk.state = 'wait'; emit('arrived', pk.id); gateState.set('pending'); }
        } else if (pk.state === 'wait') {
          pk.g.position.copy(GATE).add(p0.set(0, 0.12 * Math.sin(U.uTime.value * 3), 0));
        } else if (pk.state === 'pass') {
          pk.s = Math.min(1, pk.s + dt / (S.reduced ? 0.4 : 1.15));
          const e = ease(pk.s);
          pk.g.position.lerpVectors(pk.from, pk.target, e);
          if (pk.s >= 1) {
            aT[pk.landing] = Math.max(U.uTime.value, 0.001);
            aTAttr.needsUpdate = true;
            fx.burst(pk.target, '#7ee8c0', 46, 2.4, 1.2, 2.8);
            fx.burst(pk.target, '#ffffff', 8, 1.2, 0.7, 4);
            group.remove(pk.g);
            packets.splice(i, 1);
            continue;
          }
        }
        // look of the packet
        const hot = pk.state === 'pass' ? new Color(0.35, 1.0, 0.72) : pk.state === 'wait' && sim.mode === 'build' ? new Color(1.0, 0.64, 0.23) : new Color(0.55, 0.42, 1.0);
        pk.glow.uniforms.uColor.value.lerp(hot, 1 - Math.exp(-10 * dt));
        pk.glow.uniforms.uSize.value = 0.62 + 0.12 * Math.sin(U.uTime.value * 6 + pk.id);
        pk.core.rotation.x += dt * 2.2; pk.core.rotation.y += dt * 3;
        if (pk.state !== 'wait' && Math.random() < 0.9) fx.trail(pk.g.position, pk.state === 'pass' ? '#7ee8c0' : '#a99af7');
      }
    },
  };
}
