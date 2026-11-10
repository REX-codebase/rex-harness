// The entity's heart: a domain-warped plasma sphere with white-hot ridged veins,
// a bright limb, a billboarded corona with radial rays, and a faint outer shell.
// uTier shifts it from violet-silver to Ultra champagne.

import {
  Group, Mesh, SphereGeometry, ShaderMaterial, PlaneGeometry, AdditiveBlending, BackSide, FrontSide,
} from 'three';
import { NOISE } from './glsl.js';
import { U } from './state.js';

const CORE_VERT = /* glsl */ `
varying vec3 vObj; varying vec3 vN; varying vec3 vV;
void main(){
  vObj = position;
  vec4 mv = modelViewMatrix * vec4(position, 1.);
  vN = normalize(normalMatrix * normal);
  vV = normalize(-mv.xyz);
  gl_Position = projectionMatrix * mv;
}`;

const CORE_FRAG = /* glsl */ `
precision highp float;
uniform float uTime, uTier, uPulse, uAlive;
varying vec3 vObj; varying vec3 vN; varying vec3 vV;
${NOISE}
void main(){
  vec3 N = normalize(vN), V = normalize(vV);
  float ndv = clamp(dot(N, V), 0., 1.);
  float fr = pow(1. - ndv, 2.6);
  vec3 p = normalize(vObj);
  float t = uTime * .09;
  vec3 q = p * 1.85;
  vec3 w = vec3(snoise(q * .9 + vec3(0., t, 0.)), snoise(q * .9 + vec3(4.1, t, 2.2)), snoise(q * .9 + vec3(1.7, 3.3, t)));
  float a = snoise(q * 1.5 + w * 1.15 + vec3(0., -t * 1.6, 0.));
  float b = snoise(q * 3.3 + w * 1.9 + vec3(t * 1.1, 0., 0.));
  float heat = pow(clamp(.50 + .50 * a + .16 * b, 0., 1.), 1.25);
  float ridge = 1. - abs(snoise(q * 2.3 + w * 1.5 + vec3(0., t * .8, 0.)));
  float veins = pow(max(ridge, 0.), 8.);
  float veins2 = pow(max(1. - abs(snoise(q * 4.6 + w * 2.2 - vec3(t, 0., 0.))), 0.), 12.);

  vec3 deepV = vec3(.07, .03, .36), midV = vec3(.40, .24, 1.0), hotV = vec3(.86, .78, 1.0);
  vec3 deepG = vec3(.28, .12, .03), midG = vec3(.96, .62, .24), hotG = vec3(1.0, .94, .80);
  vec3 deep = mix(deepV, deepG, uTier), mid = mix(midV, midG, uTier), hot = mix(hotV, hotG, uTier);

  vec3 col = mix(deep, mid, smoothstep(.10, .64, heat));
  col = mix(col, hot, smoothstep(.72, 1., heat));
  col *= .40 + 1.05 * heat * heat;
  col += hot * (veins * 1.35 + veins2 * .65);
  col *= mix(.28, 1., pow(ndv, .62));                                         // limb darkening, like a real star
  col += mix(vec3(.60, .48, 1.), vec3(1., .78, .46), uTier) * fr * .55;       // thin rim light
  col *= (1. + uPulse) * mix(.0, 1., uAlive);
  gl_FragColor = vec4(col, 1.);
}`;

const SHELL_FRAG = /* glsl */ `
uniform float uTier, uAlive;
varying vec3 vN; varying vec3 vV;
void main(){
  float ndv = clamp(dot(normalize(vN), normalize(vV)), 0., 1.);
  float b = 1.34 * sqrt(max(1. - ndv * ndv, 0.));            // impact parameter in core radii
  float g = exp(-(b - 1.) * 6.5) * step(1., b) * smoothstep(1.34, 1.12, b);
  vec3 c = mix(vec3(.50, .36, 1.), vec3(1., .72, .34), uTier);
  gl_FragColor = vec4(c * g * 1.25 * uAlive, g);
}`;

const CORONA_VERT = /* glsl */ `
uniform float uSize; varying vec2 vUv;
void main(){
  vUv = uv * 2. - 1.;
  vec4 mv = modelViewMatrix * vec4(0., 0., 0., 1.);
  mv.xy += position.xy * uSize;
  gl_Position = projectionMatrix * mv;
}`;

const CORONA_FRAG = /* glsl */ `
uniform float uTime, uTier, uPulse, uAlive;
varying vec2 vUv;
${NOISE}
void main(){
  float r = length(vUv);
  float ang = atan(vUv.y, vUv.x);
  float glow = (exp(-pow(r * 2.05, 1.15) * 1.9) * .55 + exp(-r * 6.5) * .75) * 1.0;
  float s1 = snoise(vec3(cos(ang) * 2.2, sin(ang) * 2.2, uTime * .07)) * .5 + .5;
  float s2 = snoise(vec3(cos(ang) * 5.5, sin(ang) * 5.5, uTime * .11 + 9.)) * .5 + .5;
  float rays = (pow(s1, 3.) * .8 + pow(s2, 4.) * .5) * exp(-r * 3.0) * .85;
  float edge = smoothstep(1., .62, r);
  vec3 c = mix(vec3(.46, .34, 1.), vec3(1., .70, .32), uTier);
  vec3 hot = mix(vec3(.88, .82, 1.), vec3(1., .94, .80), uTier);
  vec3 col = (c * (glow + rays) + hot * exp(-r * 10.) * .35) * edge * (1. + uPulse * 1.5) * uAlive;
  gl_FragColor = vec4(col, 1.);
}`;

export function createCore() {
  const group = new Group();
  group.name = 'core';
  group.scale.setScalar(1.2);

  const sphere = new Mesh(
    new SphereGeometry(1, 128, 96),
    new ShaderMaterial({
      uniforms: { uTime: U.uTime, uTier: U.uTier, uPulse: U.uPulse, uAlive: U.uAlive },
      vertexShader: CORE_VERT,
      fragmentShader: CORE_FRAG,
      side: FrontSide,
    }),
  );
  group.add(sphere);

  const shell = new Mesh(
    new SphereGeometry(1.34, 64, 48),
    new ShaderMaterial({
      uniforms: { uTier: U.uTier, uAlive: U.uAlive },
      vertexShader: CORE_VERT,
      fragmentShader: SHELL_FRAG,
      transparent: true,
      blending: AdditiveBlending,
      depthWrite: false,
      side: FrontSide,
    }),
  );
  group.add(shell);

  const coronaMat = new ShaderMaterial({
    uniforms: { uTime: U.uTime, uTier: U.uTier, uPulse: U.uPulse, uAlive: U.uAlive, uSize: { value: 5.2 } },
    vertexShader: CORONA_VERT,
    fragmentShader: CORONA_FRAG,
    transparent: true,
    blending: AdditiveBlending,
    depthWrite: false,
  });
  const corona = new Mesh(new PlaneGeometry(2, 2), coronaMat);
  corona.frustumCulled = false;
  group.add(corona);

  return {
    object: group,
    corona: coronaMat,
    update(dt, t) {
      sphere.rotation.y = t * 0.03;
      coronaMat.uniforms.uSize.value = 5.2 * (1 + 0.025 * Math.sin(t * 0.7));
    },
  };
}
