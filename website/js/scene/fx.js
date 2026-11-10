// A small pool of GPU sparks for bursts (approvals, rejections, eliminations)
// and comet tails. Motion is computed in the vertex shader from a spawn time, so
// a burst costs one attribute upload and nothing per frame.

import { Points, BufferGeometry, BufferAttribute, ShaderMaterial, AdditiveBlending, Color, Vector3 } from 'three';
import { U } from './state.js';

const VERT = /* glsl */ `
attribute vec3 aVel; attribute vec4 aMeta; attribute vec3 aCol;
uniform float uTime, uPR;
varying vec3 vCol; varying float vA;
void main(){
  float age = uTime - aMeta.x;
  float life = aMeta.y;
  if (age < 0. || age > life) { gl_Position = vec4(2., 2., 2., 1.); gl_PointSize = 0.; vA = 0.; vCol = vec3(0.); return; }
  float k = age / life;
  vec3 p = position + aVel * age * (1. - .45 * k);
  vec4 mv = modelViewMatrix * vec4(p, 1.);
  gl_Position = projectionMatrix * mv;
  gl_PointSize = max(aMeta.z * uPR * (28. / max(-mv.z, 1.)) * (1. - k * .55), 1.);
  vA = (1. - k) * (1. - k);
  vCol = aCol;
}`;

const FRAG = /* glsl */ `
varying vec3 vCol; varying float vA;
void main(){
  vec2 p = gl_PointCoord * 2. - 1.;
  float a = exp(-dot(p, p) * 3.6) * vA;
  if (a < .004) discard;
  gl_FragColor = vec4(vCol * a * 2.2, 1.);
}`;

export function createFX(max = 700) {
  const pos = new Float32Array(max * 3), vel = new Float32Array(max * 3), meta = new Float32Array(max * 4), col = new Float32Array(max * 3);
  for (let i = 0; i < max; i++) meta[i * 4] = -1e6;
  const g = new BufferGeometry();
  const aPos = new BufferAttribute(pos, 3), aVel = new BufferAttribute(vel, 3), aMeta = new BufferAttribute(meta, 4), aCol = new BufferAttribute(col, 3);
  g.setAttribute('position', aPos);
  g.setAttribute('aVel', aVel);
  g.setAttribute('aMeta', aMeta);
  g.setAttribute('aCol', aCol);
  const pts = new Points(g, new ShaderMaterial({
    uniforms: { uTime: U.uTime, uPR: U.uPR },
    vertexShader: VERT, fragmentShader: FRAG, transparent: true, blending: AdditiveBlending, depthWrite: false,
  }));
  pts.frustumCulled = false;
  let head = 0;
  const c = new Color();

  return {
    object: pts,
    // n sparks leaving `p` in random directions at about `speed` units/s
    burst(p, color, n = 40, speed = 3, life = 1.1, size = 3) {
      c.set(color);
      for (let i = 0; i < n; i++) {
        const j = head++ % max;
        const th = Math.random() * Math.PI * 2, z = Math.random() * 2 - 1, s = Math.sqrt(1 - z * z), v = speed * (0.35 + Math.random() * 0.85);
        pos.set([p.x, p.y, p.z], j * 3);
        vel.set([s * Math.cos(th) * v, z * v, s * Math.sin(th) * v], j * 3);
        meta.set([U.uTime.value, life * (0.6 + Math.random() * 0.7), size * (0.6 + Math.random() * 0.8), 0], j * 4);
        col.set([c.r, c.g, c.b], j * 3);
      }
      aPos.needsUpdate = aVel.needsUpdate = aMeta.needsUpdate = aCol.needsUpdate = true;
    },
    // a couple of slow sparks trailing from a moving point
    trail(p, color, v = new Vector3()) {
      c.set(color);
      const j = head++ % max;
      pos.set([p.x, p.y, p.z], j * 3);
      vel.set([v.x + (Math.random() - 0.5) * 0.4, v.y + (Math.random() - 0.5) * 0.4, v.z + (Math.random() - 0.5) * 0.4], j * 3);
      meta.set([U.uTime.value, 0.7, 2.2, 0], j * 4);
      col.set([c.r, c.g, c.b], j * 3);
      aPos.needsUpdate = aVel.needsUpdate = aMeta.needsUpdate = aCol.needsUpdate = true;
    },
  };
}
