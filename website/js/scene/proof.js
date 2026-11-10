// Chapter 03: the ledger. Every run emits a signed receipt; here each receipt is
// an iridescent crystal on a helix, joined by a chain. As the reader scrolls, a
// verification front climbs the column and turns each crystal from violet to
// mint: `rex verify` and `rex replay` re-checking, offline, one at a time.

import {
  Group, Mesh, InstancedMesh, OctahedronGeometry, MeshPhysicalMaterial, InstancedBufferAttribute, Matrix4, Quaternion, Vector3, Euler, Color,
  CatmullRomCurve3, TubeGeometry, ShaderMaterial, AdditiveBlending, TorusGeometry, Points, BufferGeometry, BufferAttribute, DynamicDrawUsage,
  CylinderGeometry, DoubleSide,
} from 'three';
import { U, S, TAU, clamp, smooth, lerp } from './state.js';
import { LEDGER } from './rig.js';

const N = 16, Y0 = -5.2, Y1 = 5.8, R = 2.35, TURNS = 1.7;
const helixPoint = (i, out) => {
  const f = i / (N - 1);
  const a = f * TURNS * TAU;
  return out.set(Math.cos(a) * R, lerp(Y0, Y1, f), Math.sin(a) * R);
};

const CHAIN_VERT = 'varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }';
const CHAIN_FRAG = /* glsl */ `
uniform float uTime, uSweep, uVis;
varying vec2 vUv;
void main(){
  float s = vUv.x;
  float ok = 1. - smoothstep(uSweep - .01, uSweep + .01, s);
  vec3 violet = vec3(.50, .38, 1.), mint = vec3(.30, 1., .72);
  vec3 c = mix(violet, mint, ok);
  float pulse = exp(-pow((fract(s * 5. - uTime * .35) - .5) * 7., 2.));
  float a = (.18 + .9 * pulse) * uVis;
  gl_FragColor = vec4(c * a * 1.6, a);
}`;

const STREAM_VERT = /* glsl */ `
attribute float aPh; uniform float uTime, uPR, uVis; uniform vec3 uP0, uP1, uP2, uP3; varying float vA;
vec3 bez(float t){ float u = 1. - t; return u*u*u*uP0 + 3.*u*u*t*uP1 + 3.*u*t*t*uP2 + t*t*t*uP3; }
void main(){
  float t = fract(aPh + uTime * .11);
  vec3 p = bez(t);
  vec4 mv = modelViewMatrix * vec4(p, 1.);
  gl_Position = projectionMatrix * mv;
  gl_PointSize = max((1.6 + 2.2 * sin(t * 3.14159)) * uPR * (26. / max(-mv.z, 1.)), 1.4 * uPR);
  vA = sin(t * 3.14159) * uVis;
}`;
const STREAM_FRAG = 'varying float vA; void main(){ vec2 p = gl_PointCoord * 2. - 1.; float a = exp(-dot(p,p) * 3.2) * vA; if (a < .01) discard; gl_FragColor = vec4(vec3(.62, .50, 1.) * a * 1.8, 1.); }';

export function createProof({ labels, envMap }) {
  const group = new Group();
  group.name = 'proof';
  group.visible = false;
  group.position.set(LEDGER.x, 0, LEDGER.z);

  // crystals
  const mat = new MeshPhysicalMaterial({
    color: 0x1b1733, metalness: 0.0, roughness: 0.07, clearcoat: 1, clearcoatRoughness: 0.04,
    iridescence: 1, iridescenceIOR: 1.55, iridescenceThicknessMinimum: 180, iridescenceThicknessMaximum: 640, envMap, envMapIntensity: 3.2,
    emissive: new Color(0x000000), emissiveIntensity: 1,
  });
  mat.onBeforeCompile = (sh) => {
    sh.vertexShader = sh.vertexShader.replace('#include <common>', '#include <common>\nattribute float aV; varying float vV;').replace('#include <begin_vertex>', '#include <begin_vertex>\nvV = aV;');
    sh.fragmentShader = sh.fragmentShader.replace('#include <common>', '#include <common>\nvarying float vV;').replace('#include <emissivemap_fragment>', '#include <emissivemap_fragment>\ntotalEmissiveRadiance += mix(vec3(.16, .10, .50), vec3(.10, .95, .62), vV) * (.45 + 1.5 * vV);');
  };
  const geo = new OctahedronGeometry(0.5, 0);
  const crystals = new InstancedMesh(geo, mat, N);
  const aV = new Float32Array(N);
  const aVAttr = new InstancedBufferAttribute(aV, 1);
  aVAttr.setUsage(DynamicDrawUsage);
  geo.setAttribute('aV', aVAttr);
  crystals.frustumCulled = false;
  group.add(crystals);

  // chain
  const pts = [];
  for (let i = 0; i < N; i++) pts.push(helixPoint(i, new Vector3()));
  const curve = new CatmullRomCurve3(pts, false, 'catmullrom', 0.5);
  const chainMat = new ShaderMaterial({ uniforms: { uTime: U.uTime, uSweep: { value: 0 }, uVis: { value: 0 } }, vertexShader: CHAIN_VERT, fragmentShader: CHAIN_FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false });
  group.add(new Mesh(new TubeGeometry(curve, 360, 0.014, 6, false), chainMat));

  // verification front
  const scanMat = new ShaderMaterial({
    uniforms: { uVis: { value: 0 } },
    vertexShader: 'void main(){ gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }',
    fragmentShader: 'uniform float uVis; void main(){ gl_FragColor = vec4(vec3(.30, 1., .72) * 2.4 * uVis, uVis); }',
    transparent: true, blending: AdditiveBlending, depthWrite: false,
  });
  const scan = new Mesh(new TorusGeometry(R + 0.75, 0.018, 8, 128), scanMat);
  scan.rotation.x = Math.PI / 2;
  group.add(scan);
  const discMat = new ShaderMaterial({
    uniforms: { uVis: { value: 0 } },
    vertexShader: 'varying vec2 vP; void main(){ vP = position.xy; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }',
    fragmentShader: 'uniform float uVis; varying vec2 vP; void main(){ float r = length(vP) / 3.1; float a = (1. - smoothstep(.2, 1., r)) * .10 * uVis; gl_FragColor = vec4(vec3(.30, 1., .72) * a * 1.6, a); }',
    transparent: true, blending: AdditiveBlending, depthWrite: false, side: DoubleSide,
  });
  const disc = new Mesh(new CylinderGeometry(3.1, 3.1, 0.001, 64, 1, false), discMat);
  disc.geometry.rotateX(Math.PI / 2);
  disc.rotation.x = Math.PI / 2;
  scan.add(disc);

  // a stream of sparks from the entity into the bottom of the ledger
  const SN = 140;
  const ph = new Float32Array(SN);
  const pos0 = new Float32Array(SN * 3);
  for (let i = 0; i < SN; i++) ph[i] = i / SN;
  const sg = new BufferGeometry();
  sg.setAttribute('position', new BufferAttribute(pos0, 3));
  sg.setAttribute('aPh', new BufferAttribute(ph, 1));
  const entry = helixPoint(0, new Vector3());
  const stream = new Points(sg, new ShaderMaterial({
    uniforms: {
      uTime: U.uTime, uPR: U.uPR, uVis: { value: 0 },
      uP0: { value: new Vector3(-LEDGER.x, 0, -LEDGER.z) },
      uP1: { value: new Vector3(-LEDGER.x * 0.55, -1.0, -LEDGER.z * 0.6 + 2.2) },
      uP2: { value: new Vector3(-LEDGER.x * 0.12, -4.9, entry.z + 2.0) },
      uP3: { value: entry.clone() },
    },
    vertexShader: STREAM_VERT, fragmentShader: STREAM_FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false,
  }));
  stream.frustumCulled = false;
  group.add(stream);

  // labels
  const worldAt = (i) => (out) => helixPoint(i, out).add(group.position);
  const lStart = labels.add({ text: 'Signed receipt', sub: 'Ed25519', anchor: worldAt(2), chapter: 'proof', dx: -110, dy: 0 });
  const lScan = labels.add({ text: 'rex verify', sub: 'offline', anchor: (out) => out.set(0, scan.position.y, 0).add(group.position), chapter: 'proof', dx: 0, dy: 0, cls: 'is-ok' });
  lScan.el.style.marginLeft = '-170px';

  const m = new Matrix4(), q = new Quaternion(), e = new Euler(), p = new Vector3(), s = new Vector3();
  const sweepPrev = { v: 0 };

  return {
    object: group,
    update(dt, t) {
      const w = S.w.proof || 0;
      group.visible = w > 0.01;
      if (!group.visible) return;
      const pr = S.p.proof || 0;
      const sweepN = smooth(0.22, 0.88, pr); // 0..1 up the column
      const front = sweepN * (N + 1) - 0.5;
      for (let i = 0; i < N; i++) {
        helixPoint(i, p);
        const pop = smooth(0, 0.7, w * 1.4 - (i / N) * 0.35);
        const k = pop * (1 + 0.04 * Math.sin(t * 1.3 + i));
        s.set(0.62 * k, 1.35 * k, 0.62 * k);
        e.set(Math.sin(t * 0.3 + i) * 0.15, t * (0.25 + (i % 3) * 0.07) + i, Math.cos(t * 0.27 + i) * 0.15);
        q.setFromEuler(e);
        m.compose(p, q, s);
        crystals.setMatrixAt(i, m);
        aV[i] = clamp(front - i + 0.5);
      }
      crystals.instanceMatrix.needsUpdate = true;
      aVAttr.needsUpdate = true;

      const y = lerp(Y0 - 0.4, Y1 + 0.4, sweepN);
      scan.position.y = y;
      scanMat.uniforms.uVis.value = w * smooth(0.0, 0.05, sweepN) * (1 - smooth(0.97, 1, sweepN) * 0.8);
      discMat.uniforms.uVis.value = scanMat.uniforms.uVis.value;
      chainMat.uniforms.uVis.value = w;
      chainMat.uniforms.uSweep.value = sweepN;
      stream.material.uniforms.uVis.value = w;
      lScan.alpha = smooth(0.0, 0.08, sweepN) * (1 - smooth(0.9, 1.0, sweepN));
      lStart.alpha = 1 - sweepN * 0.6;
    },
  };
}
