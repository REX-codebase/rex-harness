// Chapter 04: Ultra, as a diagram you can scrub. Everything here is a pure
// function of scroll progress, so it plays forward and backward:
//
//   0.00  contract frozen        a ring of engraved text seals the entity
//   0.20  four candidates fork   from one immutable state, out to orbit
//   0.40  the adversary attacks  a separate run finds a concrete defect (B)
//   0.62  the verifier sweeps    deterministic re-execution: C's proof is stale;
//                                the clean-room judge rejects D
//   0.80  promotion              the one survivor (A) is promoted into the entity
//
// The choreography illustrates docs/ultra-mode.md. It is not a recording of a run.

import {
  Group, Mesh, SphereGeometry, ShaderMaterial, IcosahedronGeometry, MeshStandardMaterial, EdgesGeometry, LineSegments, LineBasicMaterial,
  CylinderGeometry, MeshBasicMaterial, TorusGeometry, RingGeometry, PlaneGeometry, Color, Vector3, AdditiveBlending, DoubleSide,
} from 'three';
import { U, S, TAU, clamp, smooth, lerp } from './state.js';
import { NOISE } from './glsl.js';
import { createFX } from './fx.js';
import { bandTexture } from './rings.js';

const PLANET_VERT = /* glsl */ `
varying vec3 vObj; varying vec3 vN; varying vec3 vW; varying vec3 vV;
void main(){
  vObj = position;
  vec4 wp = modelMatrix * vec4(position, 1.);
  vW = wp.xyz;
  vN = normalize(mat3(modelMatrix) * normal);
  vV = normalize(cameraPosition - wp.xyz);
  gl_Position = projectionMatrix * viewMatrix * wp;
}`;
const PLANET_FRAG = /* glsl */ `
precision highp float;
uniform vec3 uA, uB, uEdge; uniform float uSeed, uDissolve, uWin, uPing, uVis, uTier, uTime;
varying vec3 vObj; varying vec3 vN; varying vec3 vW; varying vec3 vV;
${NOISE}
void main(){
  vec3 N = normalize(vN), V = normalize(vV);
  float dn = snoise(vObj * 2.6 + uSeed * 7.) * .5 + .5;
  if (dn < uDissolve) discard;
  float edge = smoothstep(uDissolve + .09, uDissolve, dn) * step(.001, uDissolve);
  float n = fbm3(vObj * 2.1 + uSeed * 3. + vec3(0., uTime * .02, 0.));
  float bands = sin(vObj.y * 9. + n * 5. + uSeed * 10.) * .5 + .5;
  vec3 albedo = mix(uA, uB, smoothstep(-.35, .5, n) * .7 + bands * .3);
  vec3 L = normalize(-vW);
  float ndl = max(dot(N, L), 0.);
  float fr = pow(1. - max(dot(N, V), 0.), 3.);
  vec3 col = albedo * (.05 + 1.7 * ndl);
  col += mix(uB, vec3(.7, .6, 1.), .4) * fr * (.25 + ndl * .9);
  vec3 gold = mix(vec3(.9, .85, 1.), vec3(1., .82, .5), uTier);
  col += gold * (uWin * (.55 + fr * 2.2) + uPing * (.4 + fr * 1.6));
  col += uEdge * edge * 5.;
  gl_FragColor = vec4(col * uVis, 1.);
}`;

const GLOW_VERT = 'uniform float uSize; varying vec2 vUv; void main(){ vUv = uv * 2. - 1.; vec4 mv = modelViewMatrix * vec4(0., 0., 0., 1.); mv.xy += position.xy * uSize; gl_Position = projectionMatrix * mv; }';
const GLOW_FRAG = 'uniform vec3 uColor; uniform float uVis; varying vec2 vUv; void main(){ float r = length(vUv); float a = (exp(-r * 4.) + exp(-r * 13.) * 1.5) * uVis; gl_FragColor = vec4(uColor * a, a); }';

const CANDS = [
  { id: 'A', a: [0.34, 0.30, 0.78], b: [0.82, 0.78, 1.0], edge: [1, 0.8, 0.4] },
  { id: 'B', a: [0.10, 0.28, 0.62], b: [0.36, 0.62, 1.0], edge: [1.0, 0.42, 0.30] },
  { id: 'C', a: [0.10, 0.42, 0.42], b: [0.36, 0.86, 0.72], edge: [1.0, 0.45, 0.25] },
  { id: 'D', a: [0.52, 0.18, 0.46], b: [0.95, 0.52, 0.78], edge: [0.8, 0.5, 1.0] },
];
const ORBIT = 7.4;
const ease = (x) => x * x * (3 - 2 * x);
const seg = (p, a, b) => clamp((p - a) / (b - a));

export function createUltra({ labels }) {
  const group = new Group();
  group.name = 'ultra';
  group.visible = false;
  const fx = createFX(900);
  group.add(fx.object);

  // contract seal
  const sealTex = bandTexture('ACCEPTANCE CONTRACT', 'FROZEN');
  const sealMat = new ShaderMaterial({
    uniforms: { tMap: { value: sealTex }, uVis: { value: 0 }, uTier: U.uTier },
    vertexShader: 'varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }',
    fragmentShader: 'uniform sampler2D tMap; uniform float uVis; varying vec2 vUv; void main(){ vec4 t = texture2D(tMap, vec2(vUv.x * 2., vUv.y)); float e = smoothstep(0., .15, vUv.y) * smoothstep(1., .85, vUv.y); vec3 c = vec3(1., .86, .56) * 1.7 * t.a * e * uVis; gl_FragColor = vec4(c, t.a * e * uVis); }',
    transparent: true, blending: AdditiveBlending, depthWrite: false, side: DoubleSide,
  });
  const seal = new Mesh(new CylinderGeometry(2.0, 2.0, 0.5, 160, 1, true), sealMat);
  group.add(seal);
  const sealRing = new Mesh(new TorusGeometry(2.0, 0.012, 8, 160), new MeshBasicMaterial({ color: new Color(2.2, 1.7, 1.0), toneMapped: false, transparent: true, opacity: 0, blending: AdditiveBlending, depthWrite: false }));
  sealRing.rotation.x = Math.PI / 2;
  group.add(sealRing);

  // candidates
  const planets = CANDS.map((c, i) => {
    const mat = new ShaderMaterial({
      uniforms: {
        uA: { value: new Color(...c.a) }, uB: { value: new Color(...c.b) }, uEdge: { value: new Color(...c.edge) },
        uSeed: { value: i * 1.7 + 0.3 }, uDissolve: { value: 0 }, uWin: { value: 0 }, uPing: { value: 0 }, uVis: { value: 0 }, uTier: U.uTier, uTime: U.uTime,
      },
      vertexShader: PLANET_VERT, fragmentShader: PLANET_FRAG,
    });
    const mesh = new Mesh(new SphereGeometry(0.9, 72, 56), mat);
    const glowMat = new ShaderMaterial({ uniforms: { uColor: { value: new Color(...c.b).multiplyScalar(0.5) }, uVis: { value: 0 }, uSize: { value: 2.0 } }, vertexShader: GLOW_VERT, fragmentShader: GLOW_FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false });
    const glow = new Mesh(new PlaneGeometry(2, 2), glowMat);
    glow.frustumCulled = false;
    const g = new Group();
    g.add(mesh, glow);
    group.add(g);
    const lab = labels.add({ text: c.id, sub: 'building', anchor: g, chapter: 'ultra', dy: -38 });
    return { c, g, mesh, mat, glowMat, lab, dead: false, state: '' };
  });

  // adversary
  const adv = new Group();
  const advBody = new Mesh(new IcosahedronGeometry(0.6, 0), new MeshStandardMaterial({ color: 0x1a1018, metalness: 0.9, roughness: 0.3, emissive: new Color(0.5, 0.08, 0.05), flatShading: true }));
  const advEdges = new LineSegments(new EdgesGeometry(new IcosahedronGeometry(0.61, 0)), new LineBasicMaterial({ color: new Color(1.0, 0.45, 0.2), transparent: true, opacity: 1, blending: AdditiveBlending, depthWrite: false }));
  const advGlowMat = new ShaderMaterial({ uniforms: { uColor: { value: new Color(1.0, 0.35, 0.18) }, uVis: { value: 0 }, uSize: { value: 1.6 } }, vertexShader: GLOW_VERT, fragmentShader: GLOW_FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false });
  const advGlow = new Mesh(new PlaneGeometry(2, 2), advGlowMat);
  advGlow.frustumCulled = false;
  adv.add(advBody, advEdges, advGlow);
  group.add(adv);
  const beamMat = new MeshBasicMaterial({ color: new Color(3, 0.9, 0.4), toneMapped: false, transparent: true, opacity: 0, blending: AdditiveBlending, depthWrite: false });
  const beam = new Mesh(new CylinderGeometry(0.03, 0.03, 1, 8, 1, true), beamMat);
  group.add(beam);
  const labAdv = labels.add({ text: 'Adversary', anchor: adv, chapter: 'ultra', dy: -34, cls: 'is-bad' });

  // verifier sweep
  const sweepMat = new ShaderMaterial({
    uniforms: { uVis: { value: 0 } },
    vertexShader: 'varying vec2 vP; void main(){ vP = position.xy; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }',
    fragmentShader: 'uniform float uVis; varying vec2 vP; void main(){ gl_FragColor = vec4(vec3(1., .86, .56) * 2.2 * uVis, uVis); }',
    transparent: true, blending: AdditiveBlending, depthWrite: false, side: DoubleSide,
  });
  const sweep = new Mesh(new RingGeometry(0.985, 1, 160), sweepMat);
  sweep.rotation.x = -Math.PI / 2;
  group.add(sweep);
  let sweepR = 0;
  const labVer = labels.add({ text: 'Verifier', sub: 'not a model', anchor: (o) => o.set(sweepR * 0.707, 0, -sweepR * 0.707), chapter: 'ultra', cls: 'is-gold' });
  labVer.alpha = 0;

  const pos = (i, p, t, out) => {
    // radius: spawn from the core, glide out
    const fork = ease(seg(p, 0.2, 0.4));
    let rho = lerp(1.3, ORBIT, fork);
    const th = (i / 4) * TAU + t * 0.1 + fork * 1.2;
    out.set(Math.cos(th) * rho, Math.sin(th * 1.0 + i) * 0.9 * fork, Math.sin(th) * rho);
    return rho;
  };
  const v = new Vector3(), v2 = new Vector3(), dirv = new Vector3(), up = new Vector3(0, 1, 0);
  const tag = (pl, text, cls) => { if (pl.state !== text) { pl.state = text; pl.lab.el.firstChild.nextSibling.textContent = text; pl.lab.state(cls); } };
  let lastP = 0;

  return {
    object: group,
    update(dt, t) {
      const w = S.w.ultra || 0;
      group.visible = w > 0.01;
      if (!group.visible) return;
      const p = S.p.ultra || 0;

      // contract seal
      const sealIn = smooth(0.0, 0.14, p);
      sealMat.uniforms.uVis.value = w * sealIn * (1 - 0.55 * smooth(0.2, 0.3, p));
      sealRing.material.opacity = w * sealIn * 0.8;
      seal.rotation.y = t * 0.08;
      seal.scale.setScalar(lerp(1.4, 1, ease(sealIn)));
      sealRing.scale.copy(seal.scale);

      // candidates
      const appear = smooth(0.16, 0.24, p);
      planets.forEach((pl, i) => {
        const rho = pos(i, p, t, v);
        pl.g.position.copy(v);
        pl.mesh.rotation.y = t * 0.3 + i;
        pl.g.scale.setScalar(Math.max(ease(appear) * w, 0.001));
        let dis = 0, win = 0, ping = 0;
        const hitB = 0.5, hitC = 0.7, hitD = 0.76;
        if (i === 1) dis = seg(p, hitB, hitB + 0.1);
        if (i === 2) dis = seg(p, hitC, hitC + 0.08);
        if (i === 3) dis = seg(p, hitD, hitD + 0.07);
        // verifier sweeps ping every live candidate as the front crosses its orbit
        const front = lerp(1.5, 10, (((p - 0.62) / 0.16) * 3) % 1);
        if (p > 0.62 && p < 0.8 && dis < 0.01) ping = Math.exp(-Math.pow((front - rho) * 1.3, 2));
        if (i === 0) win = smooth(0.82, 0.96, p);
        pl.mat.uniforms.uDissolve.value = dis;
        pl.mat.uniforms.uWin.value = win;
        pl.mat.uniforms.uPing.value = ping * 0.7;
        pl.mat.uniforms.uVis.value = 1;
        pl.glowMat.uniforms.uVis.value = (1 - dis) * (0.55 + win) * appear * w;
        pl.glowMat.uniforms.uSize.value = 2.0 + win * 1.6;
        // labels tell the story
        pl.lab.alpha = appear * (1 - 0.5 * smooth(0.6, 1, dis)); // a verdict outlives the world it judged, but recedes
        if (i === 0) tag(pl, p > 0.82 ? 'promoted' : p > 0.62 ? 'qualified' : p > 0.4 ? 'holding up' : 'building', p > 0.82 ? 'is-gold' : '');
        if (i === 1) tag(pl, p > hitB + 0.04 ? 'defect found' : p > 0.4 ? 'under attack' : 'building', p > hitB + 0.04 ? 'is-bad' : '');
        if (i === 2) tag(pl, p > hitC + 0.02 ? 'proof stale' : p > 0.62 ? 're-executing' : p > 0.4 ? 'holding up' : 'building', p > hitC + 0.02 ? 'is-bad' : '');
        if (i === 3) tag(pl, p > hitD + 0.02 ? 'judge: rejected' : p > 0.62 ? 'under review' : p > 0.4 ? 'holding up' : 'building', p > hitD + 0.02 ? 'is-bad' : '');
        // a spark burst the moment a candidate begins to fail
        const crossed = (a) => lastP < a && p >= a;
        if ((i === 1 && crossed(hitB)) || (i === 2 && crossed(hitC)) || (i === 3 && crossed(hitD))) fx.burst(v.clone().add(group.position), i === 3 ? '#cf9bff' : '#ff7a50', 60, 2.6, 1.3, 3);
        // promotion: A glides into the entity
        if (i === 0) {
          const prom = ease(seg(p, 0.84, 0.95));
          if (prom > 0) {
            const r0 = rho * (1 - prom) + 3.1 * prom;
            const th = Math.atan2(v.z, v.x) + prom * 2.2;
            pl.g.position.set(Math.cos(th) * r0, lerp(v.y, 0.6, prom), Math.sin(th) * r0);
          }
          if (crossed(0.93)) { fx.burst(new Vector3(0, 0, 0), '#ffd9a0', 120, 5, 1.6, 4); U.uPulse.value = 1.4; }
        }
      });
      lastP = p;

      // adversary: arrives, strikes B, withdraws
      const advIn = smooth(0.36, 0.44, p) * (1 - smooth(0.58, 0.64, p));
      const strike = seg(p, 0.44, 0.52);
      adv.visible = advIn > 0.01;
      if (adv.visible) {
        const tp = planets[1].g.position;
        const home = v2.set(-5.5, 5.2, -6.5);
        const near = dirv.copy(tp).multiplyScalar(1.28).add(up.clone().multiplyScalar(0.6));
        adv.position.lerpVectors(home, near, ease(seg(p, 0.38, 0.46)));
        adv.rotation.x = t * 1.4; adv.rotation.y = t * 1.9;
        adv.scale.setScalar(advIn);
        advGlowMat.uniforms.uVis.value = advIn * 0.8;
        // beam
        const a = adv.position, b = tp;
        const len = a.distanceTo(b);
        beam.position.copy(a).add(b).multiplyScalar(0.5);
        beam.scale.set(1, len, 1);
        beam.quaternion.setFromUnitVectors(up, dirv.copy(b).sub(a).normalize());
        beamMat.opacity = advIn * smooth(0.46, 0.5, p) * (1 - smooth(0.56, 0.6, p)) * (0.7 + 0.3 * Math.sin(t * 40));
      } else beamMat.opacity = 0;
      labAdv.alpha = advIn;

      // verifier sweep rings
      const sw = p > 0.62 && p < 0.8 ? (((p - 0.62) / 0.16) * 3) % 1 : -1;
      if (sw >= 0) {
        const r = lerp(1.5, 10.5, sw);
        sweep.scale.setScalar(r);
        sweepMat.uniforms.uVis.value = w * (1 - sw) * smooth(0.0, 0.05, sw);
      } else sweepMat.uniforms.uVis.value = 0;
      labVer.alpha = sw >= 0 ? smooth(0, 0.1, sw) * (1 - smooth(0.8, 1, sw)) : 0;
      sweepR = sw >= 0 ? lerp(1.5, 10.5, sw) : 0;
    },
  };
}
