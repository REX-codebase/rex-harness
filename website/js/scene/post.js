// HDR post-processing, written for this page rather than pulled from the
// three.js examples: scene -> half-float (optionally MSAA) target -> thresholded,
// Karis-averaged dual-filter bloom -> one composite pass that adds bloom, a hint
// of chromatic aberration, vignette, ACES tone mapping, sRGB and dither.

import {
  WebGLRenderTarget, HalfFloatType, LinearFilter, RGBAFormat, ShaderMaterial, Mesh, BufferGeometry,
  Float32BufferAttribute, OrthographicCamera, Scene, Vector2, CustomBlending, OneFactor, AddEquation, NoBlending,
} from 'three';

const VERT = /* glsl */ `
varying vec2 vUv;
void main(){ vUv = uv; gl_Position = vec4(position.xy, 0., 1.); }
`;

const DOWN = /* glsl */ `
uniform sampler2D tSrc; uniform vec2 uTexel; uniform float uFirst; uniform float uThresh; uniform float uKnee;
varying vec2 vUv;
vec3 S(vec2 o){ return texture2D(tSrc, vUv + o * uTexel).rgb; }
float luma(vec3 c){ return dot(c, vec3(.2126,.7152,.0722)); }
vec3 soft(vec3 c){
  float br = max(max(c.r,c.g),c.b);
  float k = clamp(br - uThresh + uKnee, 0., 2.*uKnee);
  k = k*k / (4.*uKnee + 1e-4);
  return c * max(k, br - uThresh) / max(br, 1e-4);
}
void main(){
  vec3 a=S(vec2(-2, 2)), b=S(vec2(0, 2)), c=S(vec2(2, 2));
  vec3 d=S(vec2(-2, 0)), e=S(vec2(0, 0)), f=S(vec2(2, 0));
  vec3 g=S(vec2(-2,-2)), h=S(vec2(0,-2)), i=S(vec2(2,-2));
  vec3 j=S(vec2(-1, 1)), k=S(vec2(1, 1)), l=S(vec2(-1,-1)), m=S(vec2(1,-1));
  vec3 o;
  if (uFirst > .5) {
    // Karis average: weight each 2x2 group by 1/(1+luma) so a single hot pixel
    // cannot flicker the whole bloom.
    vec3 g0=(a+b+d+e)*.25, g1=(b+c+e+f)*.25, g2=(d+e+g+h)*.25, g3=(e+f+h+i)*.25, g4=(j+k+l+m)*.25;
    float w0=1./(1.+luma(g0)), w1=1./(1.+luma(g1)), w2=1./(1.+luma(g2)), w3=1./(1.+luma(g3)), w4=1./(1.+luma(g4));
    o = (g0*w0*.125 + g1*w1*.125 + g2*w2*.125 + g3*w3*.125 + g4*w4*.5) / (w0*.125+w1*.125+w2*.125+w3*.125+w4*.5);
    o = soft(o);
  } else {
    o = e*.125 + (a+c+g+i)*.03125 + (b+d+f+h)*.0625 + (j+k+l+m)*.125;
  }
  gl_FragColor = vec4(o, 1.);
}
`;

const UP = /* glsl */ `
uniform sampler2D tSrc; uniform vec2 uTexel; uniform float uRadius;
varying vec2 vUv;
void main(){
  vec2 t = uTexel * uRadius;
  vec3 s  = texture2D(tSrc, vUv).rgb * 4.;
  s += texture2D(tSrc, vUv + vec2(-t.x, 0.)).rgb * 2.;
  s += texture2D(tSrc, vUv + vec2( t.x, 0.)).rgb * 2.;
  s += texture2D(tSrc, vUv + vec2(0., -t.y)).rgb * 2.;
  s += texture2D(tSrc, vUv + vec2(0.,  t.y)).rgb * 2.;
  s += texture2D(tSrc, vUv + vec2(-t.x,-t.y)).rgb;
  s += texture2D(tSrc, vUv + vec2( t.x,-t.y)).rgb;
  s += texture2D(tSrc, vUv + vec2(-t.x, t.y)).rgb;
  s += texture2D(tSrc, vUv + vec2( t.x, t.y)).rgb;
  gl_FragColor = vec4(s / 16., 1.);
}
`;

const COMPOSITE = /* glsl */ `
uniform sampler2D tScene; uniform sampler2D tBloom;
uniform vec2 uRes; uniform float uBloom, uTime, uCA, uVig, uGrain, uExposure, uTier, uFade, uFlash;
varying vec2 vUv;
vec3 aces(vec3 x){
  const float a=2.51, b=.03, c=2.43, d=.59, e=.14;
  return clamp((x*(a*x+b))/(x*(c*x+d)+e), 0., 1.);
}
float hash(vec2 p){ vec3 p3 = fract(vec3(p.xyx) * .1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
vec3 oetf(vec3 c){ return mix(c*12.92, 1.055*pow(c, vec3(1./2.4)) - .055, step(.0031308, c)); }
void main(){
  vec2 cc = vUv - .5;
  float r2 = dot(cc, cc);
  vec2 off = cc * r2 * uCA;
  vec3 col;
  col.r = texture2D(tScene, vUv + off).r;
  col.g = texture2D(tScene, vUv).g;
  col.b = texture2D(tScene, vUv - off).b;
  vec3 bl = texture2D(tBloom, vUv).rgb;
  col += bl * uBloom;
  col += uFlash * vec3(1.0, .92, 1.0) * exp(-r2 * 7.);
  col *= uExposure;
  col += mix(vec3(.0035,.0028,.0095), vec3(.0095,.0058,.0028), uTier);   // tinted black point
  col *= 1. - uVig * smoothstep(.22, .95, length(cc) * 1.38);
  col = aces(col);
  float l = dot(col, vec3(.2126,.7152,.0722));
  col = mix(vec3(l), col, 1.1);
  col = oetf(col);
  float n = hash(gl_FragCoord.xy + fract(uTime) * 71.) - .5;
  col += n * (uGrain + 1.5/255.);
  col *= uFade;
  gl_FragColor = vec4(col, 1.);
}
`;

function fsTriangle() {
  const g = new BufferGeometry();
  g.setAttribute('position', new Float32BufferAttribute([-1, -1, 0, 3, -1, 0, -1, 3, 0], 3));
  g.setAttribute('uv', new Float32BufferAttribute([0, 0, 2, 0, 0, 2], 2));
  return g;
}

export class Post {
  constructor(renderer, { msaa = 0, levels = 5 } = {}) {
    this.r = renderer;
    this.levels = levels;
    this.msaa = msaa;
    this.cam = new OrthographicCamera(-1, 1, 1, -1, 0, 1);
    this.fs = new Scene();
    this.quad = new Mesh(fsTriangle());
    this.quad.frustumCulled = false;
    this.fs.add(this.quad);

    const mk = (frag, uniforms, extra = {}) =>
      new ShaderMaterial({ vertexShader: VERT, fragmentShader: frag, uniforms, depthTest: false, depthWrite: false, ...extra });

    this.down = mk(DOWN, { tSrc: { value: null }, uTexel: { value: new Vector2() }, uFirst: { value: 0 }, uThresh: { value: 1.25 }, uKnee: { value: 0.7 } });
    this.up = mk(UP, { tSrc: { value: null }, uTexel: { value: new Vector2() }, uRadius: { value: 1.0 } }, {
      blending: CustomBlending, blendEquation: AddEquation, blendSrc: OneFactor, blendDst: OneFactor,
    });
    this.comp = mk(COMPOSITE, {
      tScene: { value: null }, tBloom: { value: null }, uRes: { value: new Vector2(1, 1) },
      uBloom: { value: 0.42 }, uTime: { value: 0 }, uCA: { value: 0.003 }, uVig: { value: 0.55 }, uGrain: { value: 0.018 },
      uExposure: { value: 1.0 }, uTier: { value: 0 }, uFade: { value: 1 }, uFlash: { value: 0 },
    });
    this.params = this.comp.uniforms;
    this.down.blending = NoBlending;
    this.rt = null;
    this.mips = [];
    this.w = 0;
    this.h = 0;
  }

  setSize(w, h) {
    w = Math.max(2, Math.floor(w));
    h = Math.max(2, Math.floor(h));
    if (w === this.w && h === this.h && this.rt) return;
    this.w = w;
    this.h = h;
    this.disposeTargets();
    this.rt = new WebGLRenderTarget(w, h, { type: HalfFloatType, format: RGBAFormat, samples: this.msaa, depthBuffer: true, minFilter: LinearFilter, magFilter: LinearFilter });
    const opts = { type: HalfFloatType, format: RGBAFormat, depthBuffer: false, minFilter: LinearFilter, magFilter: LinearFilter };
    let mw = w >> 1, mh = h >> 1;
    for (let i = 0; i < this.levels && mw >= 4 && mh >= 4; i++) {
      this.mips.push(new WebGLRenderTarget(mw, mh, opts));
      mw >>= 1;
      mh >>= 1;
    }
    this.comp.uniforms.uRes.value.set(w, h);
  }

  pass(material, target) {
    this.quad.material = material;
    this.r.setRenderTarget(target);
    this.r.render(this.fs, this.cam);
  }

  render(scene, camera, time) {
    const r = this.r;
    r.setRenderTarget(this.rt);
    r.clear();
    r.render(scene, camera);

    // down-sample chain
    let src = this.rt.texture, sw = this.w, sh = this.h;
    this.mips.forEach((m, i) => {
      this.down.uniforms.tSrc.value = src;
      this.down.uniforms.uTexel.value.set(1 / sw, 1 / sh);
      this.down.uniforms.uFirst.value = i === 0 ? 1 : 0;
      this.pass(this.down, m);
      src = m.texture;
      sw = m.width;
      sh = m.height;
    });
    // up-sample, accumulating into the next larger level
    for (let i = this.mips.length - 1; i > 0; i--) {
      const s = this.mips[i], d = this.mips[i - 1];
      this.up.uniforms.tSrc.value = s.texture;
      this.up.uniforms.uTexel.value.set(1 / s.width, 1 / s.height);
      this.up.uniforms.uRadius.value = 1.0;
      this.pass(this.up, d);
    }
    this.comp.uniforms.tScene.value = this.rt.texture;
    this.comp.uniforms.tBloom.value = this.mips[0].texture;
    this.comp.uniforms.uTime.value = time;
    this.pass(this.comp, null);
  }

  disposeTargets() {
    if (this.rt) this.rt.dispose();
    this.mips.forEach((m) => m.dispose());
    this.mips = [];
  }

  dispose() {
    this.disposeTargets();
    this.down.dispose();
    this.up.dispose();
    this.comp.dispose();
    this.quad.geometry.dispose();
  }
}
