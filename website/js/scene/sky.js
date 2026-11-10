// The cosmos behind the entity: a procedural volumetric-looking nebula, a
// galactic band, and ~10k lensed stars. The sky is analytic, so the gravitational
// lens bends the *view ray* before the nebula is sampled: the clouds and stars
// really do warp around the entity as the camera moves.

import {
  Mesh, SphereGeometry, ShaderMaterial, BackSide, Points, BufferGeometry, BufferAttribute, AdditiveBlending, Group, Scene,
  WebGLCubeRenderTarget, CubeCamera, UnsignedByteType, RGBAFormat, LinearFilter,
} from 'three';
import { NOISE, LENS } from './glsl.js';
import { U } from './state.js';

const SKY_VERT = /* glsl */ `
varying vec3 vDir;
void main(){
  vDir = position;
  vec4 p = projectionMatrix * mat4(mat3(viewMatrix)) * vec4(position, 1.);
  gl_Position = p.xyww;
}`;

// The nebula is static, so it is computed once into a cubemap (R = density of
// the cloud, G/B = two warp channels, A = galactic-band density with dust lanes)
// and sampled per pixel. The gravitational lens only remaps the sample
// direction, so the sky stays lensed at the price of a single texture fetch.
const BAKE_FRAG = /* glsl */ `
precision highp float;
varying vec3 vDir;
${NOISE}
void main(){
  vec3 d = normalize(vDir);
  vec3 q = d * 1.55;
  vec3 w = vec3(fbm3(q), fbm3(q + vec3(5.2, 1.3, 0.)), fbm3(q + vec3(2.1, 0., 8.3)));
  float n = fbm5(q * 1.25 + w * 1.35) * .5 + .5;
  float dust = fbm5(q * 2.7 + w * 2.0 + 11.) * .5 + .5;
  vec3 bandN = normalize(vec3(.22, .92, .32));
  float bd = dot(d, bandN);
  float band = exp(-bd * bd * 7.5);
  float dens = pow(smoothstep(.18, .98, n), 1.5) * (.16 + 1.15 * band);
  dens *= mix(1., .18, smoothstep(.42, .8, dust) * band);
  gl_FragColor = vec4(n, w.x * .5 + .5, w.z * .5 + .5, clamp(dens * .5, 0., 1.));
}`;

const SKY_FRAG = /* glsl */ `
precision highp float;
uniform float uTier, uThetaE; uniform vec3 uLensDir; uniform samplerCube tNeb;
varying vec3 vDir;
${LENS}
void main(){
  vec3 d = lensDir(normalize(vDir), uLensDir, uThetaE);
  vec4 s = textureCube(tNeb, d);
  float n = s.r, wx = s.g, wz = s.b, dens = s.a * 2.;
  vec3 indigo = vec3(.05, .06, .26), violet = vec3(.24, .11, .62), magenta = vec3(.60, .13, .46), teal = vec3(.02, .22, .34);
  vec3 col = mix(indigo, violet, smoothstep(.25, .85, n));
  col = mix(col, magenta, smoothstep(.50, 1., wx) * .55);
  col = mix(col, teal, smoothstep(.55, 1., wz) * .50);
  vec3 warm = mix(vec3(.30, .13, .04), vec3(.62, .30, .11), n);
  col = mix(col, warm * (.8 + .5 * n), uTier * .85);
  col *= dens * .34;
  float cd = max(dot(d, uLensDir), 0.);
  col += mix(vec3(.30, .17, .72), vec3(.70, .40, .14), uTier) * pow(cd, 26.) * .10;
  col += mix(vec3(.15, .10, .40), vec3(.40, .22, .08), uTier) * pow(cd, 3.) * .012;
  gl_FragColor = vec4(col, 1.);
}`;

const STAR_VERT = /* glsl */ `
attribute vec4 aData;
uniform float uTime, uPR, uTier, uThetaE; uniform vec3 uLensDir;
varying vec3 vCol; varying float vSpike;
${LENS}
void main(){
  vec3 d = lensDir(normalize(position), uLensDir, uThetaE);
  vec4 p = projectionMatrix * mat4(mat3(viewMatrix)) * vec4(d, 1.);
  gl_Position = p.xyww;
  float mag = aData.x, temp = aData.y, seed = aData.z;
  float tw = .84 + .16 * sin(uTime * (.8 + seed * 2.6) + seed * 80.);
  gl_PointSize = (1.3 + 6.5 * pow(mag, 1.1)) * uPR * (.92 + .08 * tw);
  vec3 cool = vec3(1., .74, .52), white = vec3(1.), hot = vec3(.60, .76, 1.);
  vec3 c = mix(mix(cool, white, smoothstep(0., .5, temp)), hot, smoothstep(.5, 1., temp));
  c = mix(c, c * vec3(1.12, .92, .68), uTier * .65);
  vCol = c * (.22 + 2.8 * pow(mag, 1.4)) * tw;
  vSpike = smoothstep(.62, .92, mag);
}`;

const STAR_FRAG = /* glsl */ `
varying vec3 vCol; varying float vSpike;
void main(){
  vec2 p = gl_PointCoord * 2. - 1.;
  float r2 = dot(p, p);
  float core = exp(-r2 * 10.);
  float halo = exp(-r2 * 2.8) * .22;
  float spike = vSpike * (exp(-abs(p.x) * 22.) * exp(-abs(p.y) * 2.4) + exp(-abs(p.y) * 22.) * exp(-abs(p.x) * 2.4)) * .55;
  float a = core + halo + spike;
  if (a < .003) discard;
  gl_FragColor = vec4(vCol * a, 1.);
}`;

function rand(seed) {
  // mulberry32: deterministic, so screenshots and the OG image are reproducible
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function createSky({ quality, renderer }) {
  const group = new Group();
  group.name = 'sky';

  // bake the nebula once
  const size = quality.name === 'high' ? 1024 : quality.name === 'medium' ? 768 : 512;
  const rt = new WebGLCubeRenderTarget(size, { type: UnsignedByteType, format: RGBAFormat, generateMipmaps: false, minFilter: LinearFilter, magFilter: LinearFilter, depthBuffer: false });
  const bakeScene = new Scene();
  const bakeMat = new ShaderMaterial({
    vertexShader: 'varying vec3 vDir; void main(){ vDir = position; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }',
    fragmentShader: BAKE_FRAG, side: BackSide,
  });
  const bakeMesh = new Mesh(new SphereGeometry(1, 32, 24), bakeMat);
  bakeScene.add(bakeMesh);
  new CubeCamera(0.1, 10, rt).update(renderer, bakeScene);
  bakeMesh.geometry.dispose();
  bakeMat.dispose();

  const sky = new Mesh(
    new SphereGeometry(1, 48, 32),
    new ShaderMaterial({
      uniforms: { tNeb: { value: rt.texture }, uTier: U.uTier, uThetaE: U.uThetaE, uLensDir: U.uLensDir },
      vertexShader: SKY_VERT,
      fragmentShader: SKY_FRAG,
      side: BackSide,
      depthWrite: false,
    }),
  );
  sky.frustumCulled = false;
  sky.renderOrder = -10;
  group.add(sky);

  // stars: denser toward the galactic band, power-law magnitudes
  const N = quality.stars;
  const pos = new Float32Array(N * 3);
  const data = new Float32Array(N * 4);
  const rnd = rand(0xc0ffee);
  const bandN = [0.22, 0.92, 0.32];
  const bl = Math.hypot(...bandN);
  let i = 0;
  while (i < N) {
    const z = rnd() * 2 - 1, a = rnd() * Math.PI * 2, rr = Math.sqrt(1 - z * z);
    const x = rr * Math.cos(a), y = z, zz = rr * Math.sin(a);
    const bd = (x * bandN[0] + y * bandN[1] + zz * bandN[2]) / bl;
    if (rnd() > 0.28 + 0.72 * Math.exp(-bd * bd * 6)) continue;
    pos.set([x, y, zz], i * 3);
    const m = Math.pow(rnd(), 7.5);
    data.set([m, Math.pow(rnd(), 0.8), rnd(), 0], i * 4);
    i++;
  }
  const g = new BufferGeometry();
  g.setAttribute('position', new BufferAttribute(pos, 3));
  g.setAttribute('aData', new BufferAttribute(data, 4));
  const stars = new Points(
    g,
    new ShaderMaterial({
      uniforms: { uTime: U.uTime, uPR: U.uPR, uTier: U.uTier, uThetaE: U.uThetaE, uLensDir: U.uLensDir },
      vertexShader: STAR_VERT,
      fragmentShader: STAR_FRAG,
      transparent: true,
      blending: AdditiveBlending,
      depthWrite: false,
    }),
  );
  stars.frustumCulled = false;
  stars.renderOrder = -9;
  group.add(stars);

  return { object: group, update() {}, dispose() { rt.dispose(); } };
}
