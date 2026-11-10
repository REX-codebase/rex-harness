// A small procedural "observatory" environment, baked once into a PMREM cube so
// the machined metal of the harness has something real to reflect: a dark
// gradient dome, two large soft light boxes and a hot spot where the core is.

import { Scene, Mesh, SphereGeometry, ShaderMaterial, BackSide, PMREMGenerator } from 'three';

const FRAG = /* glsl */ `
varying vec3 vDir;
float box(vec3 d, vec3 c, float w, float h, vec3 up){
  vec3 r = normalize(cross(up, c)); vec3 u = cross(c, r);
  float f = dot(d, c);
  if (f < .2) return 0.;
  vec2 p = vec2(dot(d, r), dot(d, u)) / f;
  vec2 e = smoothstep(vec2(w, h), vec2(w, h) * .55, abs(p));
  return e.x * e.y;
}
void main(){
  vec3 d = normalize(vDir);
  float up = d.y * .5 + .5;
  vec3 col = mix(vec3(.004, .003, .012), vec3(.030, .020, .085), pow(up, 1.4));
  col += vec3(.10, .05, .28) * exp(-pow((d.y + .05) * 5., 2.));                 // horizon band
  col += vec3(.95, .85, 1.00) * 3.2 * box(d, normalize(vec3(-.6, .55, .55)), .50, .24, vec3(0., 1., 0.));  // key light box
  col += vec3(.45, .55, 1.00) * 1.6 * box(d, normalize(vec3(.8, .25, -.5)), .40, .55, vec3(0., 1., 0.));   // cool rim box
  col += vec3(1.00, .72, .45) * 1.1 * box(d, normalize(vec3(.1, -.7, .7)), .60, .20, vec3(0., 0., 1.));    // warm kicker from below
  gl_FragColor = vec4(col, 1.);
}`;

export function createEnvironment(renderer) {
  const scene = new Scene();
  const dome = new Mesh(
    new SphereGeometry(10, 48, 32),
    new ShaderMaterial({
      vertexShader: 'varying vec3 vDir; void main(){ vDir = position; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }',
      fragmentShader: FRAG,
      side: BackSide,
    }),
  );
  scene.add(dome);
  const pmrem = new PMREMGenerator(renderer);
  const rt = pmrem.fromScene(scene, 0.02, 0.1, 100);
  pmrem.dispose();
  dome.geometry.dispose();
  dome.material.dispose();
  return rt.texture;
}
