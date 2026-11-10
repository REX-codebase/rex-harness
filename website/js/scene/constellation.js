// Chapter 05: every way to connect a model, as a constellation around the
// entity. The nodes are the same thirteen routes as the chips in the page;
// hovering or focusing a chip lights its star and the line back to the entity.

import {
  Group, PlaneGeometry, ShaderMaterial, AdditiveBlending, LineSegments, BufferGeometry, BufferAttribute, Color, Vector3,
  InstancedMesh, InstancedBufferAttribute, Matrix4,
} from 'three';
import { U, S, damp, smooth } from './state.js';
import { on } from '../core.js';

export const ROUTES = [
  ['chatgpt', 'ChatGPT plan', '#ecc98c'], ['anthropic', 'Anthropic', '#cfc6ff'], ['openai', 'OpenAI', '#cfc6ff'], ['gemini', 'Gemini', '#9fc4ff'],
  ['xai', 'xAI', '#cfc6ff'], ['deepseek', 'DeepSeek', '#9fc4ff'], ['kimi', 'Kimi', '#cfc6ff'], ['qwen', 'Qwen', '#d9a6ff'], ['glm', 'GLM', '#cfc6ff'],
  ['nim', 'NVIDIA NIM', '#9fe8c8'], ['groq', 'Groq', '#cfc6ff'], ['openrouter', 'OpenRouter', '#9fc4ff'], ['local', 'Local server', '#7ee8c0'],
];

const NODE_VERT = 'attribute float aHot; attribute vec3 aCol; uniform float uSize; varying vec2 vUv; varying float vHot; varying vec3 vCol; void main(){ vUv = uv * 2. - 1.; vHot = aHot; vCol = aCol; vec4 mv = modelViewMatrix * instanceMatrix * vec4(0., 0., 0., 1.); mv.xy += position.xy * uSize * (1. + aHot * .9); gl_Position = projectionMatrix * mv; }';
const NODE_FRAG = /* glsl */ `
uniform float uVis; varying vec2 vUv; varying float vHot; varying vec3 vCol;
void main(){
  float r = length(vUv);
  float core = exp(-r * r * 38.) * 2.4;
  float halo = exp(-r * 4.2) * .5;
  float cross = (exp(-abs(vUv.x) * 26.) * exp(-abs(vUv.y) * 3.) + exp(-abs(vUv.y) * 26.) * exp(-abs(vUv.x) * 3.)) * .55;
  float ring = exp(-pow((r - .55) * 14., 2.)) * vHot;
  float a = (core + halo + cross * (.4 + vHot) + ring * 1.4) * uVis * (.75 + .6 * vHot);
  gl_FragColor = vec4(vCol * a, a);
}`;

const LINE_VERT = 'attribute float aS; attribute float aId; varying float vS; varying float vId; void main(){ vS = aS; vId = aId; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }';
const LINE_FRAG = /* glsl */ `
uniform float uTime, uVis, uHot; varying float vS; varying float vId;
void main(){
  float hot = 1. - step(.5, abs(vId - uHot));
  float pulse = exp(-pow((fract(vS * 1.0 - uTime * (.12 + .02 * mod(vId, 5.)) + vId * .173) - .5) * 9., 2.));
  float a = (.05 + .55 * pulse * (.5 + hot * 1.5) + hot * .35) * uVis * smoothstep(0., .08, vS);
  vec3 c = mix(vec3(.55, .45, 1.), vec3(1., .92, .75), hot);
  gl_FragColor = vec4(c * a * 1.4, a);
}`;

export function createConstellation({ labels }) {
  const group = new Group();
  group.name = 'constellation';
  group.visible = false;
  const n = ROUTES.length;

  // positions: a golden-angle spiral over a cap of directions around +x (the free
  // side of the screen, away from the text panel), at varied radii for depth
  const CAP = 112 * (Math.PI / 180);
  const base = ROUTES.map((_, i) => {
    const f = (i + 0.5) / n;
    const th = Math.acos(1 - (1 - Math.cos(CAP)) * f);
    const ph = i * 2.39996 + 0.5;
    const r = 8.2 + 3.8 * ((i * 0.618034) % 1);
    const p = new Vector3(Math.cos(th) * r, Math.sin(th) * Math.cos(ph) * r * 0.82, Math.sin(th) * Math.sin(ph) * r);
    // keep every star clear of the entity as seen from the camera (which looks down -z)
    const h = Math.hypot(p.x, p.y), MIN = 7.6;
    if (h < MIN) { const k = h > 1e-3 ? MIN / h : 1; p.x = h > 1e-3 ? p.x * k : MIN; p.y *= h > 1e-3 ? k : 1; }
    return p;
  });

  // stars as instanced billboards
  const geo = new PlaneGeometry(2, 2);
  const hot = new Float32Array(n), col = new Float32Array(n * 3);
  ROUTES.forEach((r, i) => { const c = new Color(r[2]); col.set([c.r, c.g, c.b], i * 3); });
  const nodeMat = new ShaderMaterial({ uniforms: { uSize: { value: 0.95 }, uVis: { value: 0 } }, vertexShader: NODE_VERT, fragmentShader: NODE_FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false });
  const nodes = new InstancedMesh(geo, nodeMat, n);
  const hotAttr = new InstancedBufferAttribute(hot, 1);
  geo.setAttribute('aHot', hotAttr);
  geo.setAttribute('aCol', new InstancedBufferAttribute(col, 3));
  const m = new Matrix4();
  base.forEach((p, i) => { m.makeTranslation(p.x, p.y, p.z); nodes.setMatrixAt(i, m); });
  nodes.frustumCulled = false;
  group.add(nodes);

  // lines from the entity to each star, subdivided so pulses can travel
  const SEG = 28;
  const pos = new Float32Array(n * SEG * 2 * 3), aS = new Float32Array(n * SEG * 2), aId = new Float32Array(n * SEG * 2);
  base.forEach((p, i) => {
    for (let k = 0; k < SEG; k++) {
      const s0 = k / SEG, s1 = (k + 1) / SEG;
      const o = (i * SEG + k) * 6;
      const e0 = Math.pow(s0, 0.9), e1 = Math.pow(s1, 0.9);
      pos.set([p.x * e0 * 0.97, p.y * e0 * 0.97, p.z * e0 * 0.97, p.x * e1 * 0.97, p.y * e1 * 0.97, p.z * e1 * 0.97], o);
      const j = (i * SEG + k) * 2;
      aS[j] = s0; aS[j + 1] = s1; aId[j] = i; aId[j + 1] = i;
    }
  });
  const lg = new BufferGeometry();
  lg.setAttribute('position', new BufferAttribute(pos, 3));
  lg.setAttribute('aS', new BufferAttribute(aS, 1));
  lg.setAttribute('aId', new BufferAttribute(aId, 1));
  const lineMat = new ShaderMaterial({ uniforms: { uTime: U.uTime, uVis: { value: 0 }, uHot: { value: -1 } }, vertexShader: LINE_VERT, fragmentShader: LINE_FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false });
  const lines = new LineSegments(lg, lineMat);
  lines.frustumCulled = false;
  group.add(lines);

  const tags = ROUTES.map((r, i) => labels.add({ text: r[1], anchor: (o) => o.copy(base[i]), chapter: 'models', dy: 26, cls: i === 0 ? 'is-gold' : '', avoidPanel: true }));

  let hotIdx = -1;
  const off = on('provider', (id) => { hotIdx = id ? ROUTES.findIndex((r) => r[0] === id) : -1; if (hotIdx >= 0) U.uPulse.value = Math.max(U.uPulse.value, 0.35); });

  return {
    object: group,
    dispose() { off(); },
    update(dt, t) {
      const w = S.w.models || 0;
      group.visible = w > 0.01;
      if (!group.visible) return;
      nodeMat.uniforms.uVis.value = smooth(0, 1, w);
      lineMat.uniforms.uVis.value = w;
      lineMat.uniforms.uHot.value = hotIdx;
      for (let i = 0; i < n; i++) {
        hot[i] = damp(hot[i], i === hotIdx ? 1 : 0, 10, dt);
        tags[i].state(i === hotIdx ? 'is-hot' : i === 0 ? 'is-gold' : '');
        tags[i].alpha = hotIdx < 0 ? 0.78 : i === hotIdx ? 1 : 0.35;
      }
      hotAttr.needsUpdate = true;
      group.rotation.y = Math.sin(t * 0.05) * 0.06;
    },
  };
}

