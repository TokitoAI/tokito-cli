//! A self-contained, offline inspection viewer for an already validated twin.
//!
//! This presentation boundary neither generates geometry nor admits physical AR.
use serde_json::Value;

const MAX_GLB: usize = 64 * 1024 * 1024;
const MAX_INDEX: usize = 4 * 1024 * 1024;
const MAX_ELEMENTS: u64 = 20_000_000;

/// Embed actual GLB bytes and the validated selection index in a standalone HTML.
/// The caller must obtain `index` from `ValidatedTwin::index_json`, not a user edit.
pub fn generate(glb: &[u8], index: &Value) -> Result<String, String> {
    if glb.len() > MAX_GLB || glb.len() < 20 {
        return Err("viewer GLB size is outside its limit".into());
    }
    if &glb[..4] != b"glTF"
        || u32::from_le_bytes(glb[4..8].try_into().unwrap()) != 2
        || u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize != glb.len()
    {
        return Err("viewer requires a valid GLB v2 container".into());
    }
    gltf::Gltf::from_slice(glb).map_err(|e| format!("invalid viewer glTF: {e}"))?;
    let container = gltf::binary::Glb::from_slice(glb).map_err(|e| e.to_string())?;
    let document: Value = serde_json::from_slice(&container.json).map_err(|e| e.to_string())?;
    let bin = container
        .bin
        .as_ref()
        .ok_or("viewer requires an embedded BIN chunk")?;
    validate_geometry(&document, bin)?;
    if index["schema"] != "tokito.component-selection-index.v1" {
        return Err("viewer requires a component selection index v1".into());
    }
    if index["sourceSha256"]["glb"].as_str() != Some(crate::sha256_hex(glb).as_str()) {
        return Err("viewer index does not bind these GLB bytes".into());
    }
    let components = index["components"]
        .as_array()
        .ok_or("viewer index has no components")?;
    if components.len() > 10_000 {
        return Err("viewer component limit exceeded".into());
    }
    let nodes = document["nodes"].as_array().ok_or("GLB has no nodes")?;
    for component in components {
        if component["reference"].as_str().is_none() {
            return Err("viewer component is missing its reference".into());
        }
        if let Some(root) = component["selectorNodeIndex"].as_u64() {
            let name = format!("ref:{}", component["reference"].as_str().unwrap());
            if nodes.get(root as usize).is_none_or(|n| n["name"] != name) {
                return Err("viewer selector does not match its GLB node".into());
            }
        }
    }
    let json = serde_json::to_string(index).map_err(|e| e.to_string())?;
    if json.len() > MAX_INDEX {
        return Err("viewer index exceeds its byte limit".into());
    }
    // A JSON string can contain HTML script terminators even when it is valid JSON.
    let safe_json = json
        .replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    Ok(HTML
        .replace("__TOKITO_GLB__", &base64(glb))
        .replace("__TOKITO_INDEX__", &safe_json))
}

fn integer(v: &Value, key: &str) -> Result<u64, String> {
    v[key]
        .as_u64()
        .ok_or_else(|| format!("missing unsigned {key}"))
}

fn accessor<'a>(doc: &'a Value, i: &Value, bin: &[u8], channels: u64) -> Result<&'a Value, String> {
    let accessors = doc["accessors"].as_array().ok_or("GLB has no accessors")?;
    let a = i
        .as_u64()
        .and_then(|i| accessors.get(i as usize))
        .ok_or("invalid accessor index")?;
    if a.get("sparse").is_some() || a["normalized"].as_bool() == Some(true) {
        return Err("viewer does not support sparse/normalized accessors".into());
    }
    let component = integer(a, "componentType")?;
    let width = if channels > 0 {
        if component != 5126 || a["type"] != if channels == 4 { "VEC4" } else { "VEC3" } {
            return Err("viewer positions/normals require FLOAT VEC3".into());
        }
        channels * 4
    } else {
        if a["type"] != "SCALAR" {
            return Err("viewer indices require SCALAR".into());
        }
        match component {
            5121 => 1,
            5123 => 2,
            5125 => 4,
            _ => return Err("unsupported index component type".into()),
        }
    };
    let count = integer(a, "count")?;
    if count == 0 || count > MAX_ELEMENTS {
        return Err("viewer accessor count outside limits".into());
    }
    let view = a["bufferView"]
        .as_u64()
        .and_then(|i| doc["bufferViews"].as_array()?.get(i as usize))
        .ok_or("invalid buffer view")?;
    if integer(view, "buffer")? != 0 {
        return Err("viewer requires the embedded buffer".into());
    }
    let view_start = view["byteOffset"].as_u64().unwrap_or(0);
    let start = a["byteOffset"].as_u64().unwrap_or(0);
    let stride = view["byteStride"].as_u64().unwrap_or(width);
    if stride < width {
        return Err("accessor stride is too small".into());
    }
    let end = count
        .checked_sub(1)
        .and_then(|n| n.checked_mul(stride))
        .and_then(|n| n.checked_add(start))
        .and_then(|n| n.checked_add(width))
        .ok_or("accessor overflow")?;
    let length = integer(view, "byteLength")?;
    if end > length
        || view_start
            .checked_add(length)
            .is_none_or(|n| n > bin.len() as u64)
    {
        return Err("accessor exceeds its BIN/view bounds".into());
    }
    let begin = view_start + start;
    for n in 0..count {
        let offset = (begin + n * stride) as usize;
        if channels > 0 {
            for axis in 0..channels as usize {
                let value = f32::from_le_bytes(
                    bin[offset + axis * 4..offset + axis * 4 + 4]
                        .try_into()
                        .unwrap(),
                );
                if !value.is_finite()
                    || value.abs() > 1_000_000.0
                    || (channels == 4 && !(0.0..=1.0).contains(&value))
                {
                    return Err("non-finite or excessive geometry coordinate".into());
                }
            }
        }
    }
    Ok(a)
}

fn validate_geometry(doc: &Value, bin: &[u8]) -> Result<(), String> {
    for group in ["buffers", "images"] {
        if doc[group]
            .as_array()
            .is_some_and(|items| items.iter().any(|v| v.get("uri").is_some()))
        {
            return Err("viewer does not fetch external resources".into());
        }
    }
    if doc["extensionsRequired"]
        .as_array()
        .is_some_and(|x| !x.is_empty())
    {
        return Err("viewer does not support required glTF extensions".into());
    }
    if doc["animations"].as_array().is_some_and(|x| !x.is_empty()) {
        return Err("viewer supports static geometry, not animations".into());
    }
    let nodes = doc["nodes"].as_array().ok_or("GLB has no nodes")?;
    if nodes.len() > 100_000 {
        return Err("viewer node limit exceeded".into());
    }
    for n in nodes {
        if n.get("skin").is_some() || n.get("weights").is_some() {
            return Err("viewer does not support skins/morph weights".into());
        }
        if n.get("matrix").is_some()
            && ["translation", "rotation", "scale"]
                .iter()
                .any(|k| n.get(k).is_some())
        {
            return Err("node matrix and TRS cannot coexist".into());
        }
        for (key, size) in [
            ("matrix", 16),
            ("translation", 3),
            ("rotation", 4),
            ("scale", 3),
        ] {
            if let Some(v) = n.get(key) {
                if v.as_array().is_none_or(|a| {
                    a.len() != size
                        || a.iter().any(|x| {
                            x.as_f64()
                                .is_none_or(|n| !n.is_finite() || n.abs() > 1_000_000.0)
                        })
                }) {
                    return Err(format!("invalid node {key}"));
                }
            }
        }
    }
    let meshes = doc["meshes"].as_array().ok_or("GLB has no meshes")?;
    let mut total = 0u64;
    for mesh in meshes {
        for p in mesh["primitives"]
            .as_array()
            .ok_or("mesh has no primitives")?
        {
            let mode = p["mode"].as_u64().unwrap_or(4);
            if mode > 4 || p.get("targets").is_some() || p.get("extensions").is_some() {
                return Err(
                    "viewer supports uncompressed points, lines and triangle primitives without morph targets".into(),
                );
            }
            let pos = accessor(doc, &p["attributes"]["POSITION"], bin, 3)?;
            let count = integer(pos, "count")?;
            if let Some(color) = p["attributes"].get("COLOR_0") {
                if integer(accessor(doc, color, bin, 4)?, "count")? != count {
                    return Err("vertex colour count does not match positions".into());
                }
            }
            if let Some(normal) = p["attributes"].get("NORMAL") {
                if integer(accessor(doc, normal, bin, 3)?, "count")? != count {
                    return Err("normal count differs from positions".into());
                }
            }
            let triangles = if let Some(i) = p.get("indices") {
                let a = accessor(doc, i, bin, 0)?;
                let view = &doc["bufferViews"][integer(a, "bufferView")? as usize];
                let bytes = match integer(a, "componentType")? {
                    5121 => 1,
                    5123 => 2,
                    _ => 4,
                };
                let start = view["byteOffset"].as_u64().unwrap_or(0)
                    + a["byteOffset"].as_u64().unwrap_or(0);
                let stride = view["byteStride"].as_u64().unwrap_or(bytes);
                let len = integer(a, "count")?;
                for n in 0..len {
                    let o = (start + n * stride) as usize;
                    let index = match bytes {
                        1 => bin[o] as u32,
                        2 => u16::from_le_bytes(bin[o..o + 2].try_into().unwrap()) as u32,
                        _ => u32::from_le_bytes(bin[o..o + 4].try_into().unwrap()),
                    };
                    if index as u64 >= count {
                        return Err("triangle index outside positions".into());
                    }
                }
                len
            } else {
                count
            };
            if (mode == 4 && triangles % 3 != 0)
                || (mode == 1 && triangles % 2 != 0)
                || ((mode == 2 || mode == 3) && triangles < 2)
            {
                return Err("invalid primitive element count for its drawing mode".into());
            }
            total = total
                .checked_add(count + triangles)
                .ok_or("viewer geometry count overflow")?;
            if total > MAX_ELEMENTS {
                return Err("viewer total geometry budget exceeded".into());
            }
            if let Some(material) = p["material"].as_u64() {
                let m = doc["materials"]
                    .as_array()
                    .and_then(|a| a.get(material as usize))
                    .ok_or("invalid material")?;
                if let Some(color) = m["pbrMetallicRoughness"].get("baseColorFactor") {
                    if color.as_array().is_none_or(|a| {
                        a.len() != 4
                            || a.iter()
                                .any(|n| n.as_f64().is_none_or(|n| !(0.0..=1.0).contains(&n)))
                    }) {
                        return Err("invalid base-color factor".into());
                    }
                }
                if let Some(emissive) = m.get("emissiveFactor") {
                    if emissive.as_array().is_none_or(|a| {
                        a.len() != 3
                            || a.iter()
                                .any(|n| n.as_f64().is_none_or(|v| !(0.0..=1.0).contains(&v)))
                    }) {
                        return Err("invalid emissive factor".into());
                    }
                }
                for field in ["roughnessFactor", "metallicFactor"] {
                    if m["pbrMetallicRoughness"].get(field).is_some_and(|value| {
                        value
                            .as_f64()
                            .is_none_or(|value| !(0.0..=1.0).contains(&value))
                    }) {
                        return Err("invalid roughness or metallic factor".into());
                    }
                }
                if m["alphaMode"]
                    .as_str()
                    .is_some_and(|s| s != "OPAQUE" && s != "BLEND")
                    || m.get("normalTexture").is_some()
                    || m.get("emissiveTexture").is_some()
                    || m.get("occlusionTexture").is_some()
                    || m["pbrMetallicRoughness"].get("baseColorTexture").is_some()
                    || m["pbrMetallicRoughness"]
                        .get("metallicRoughnessTexture")
                        .is_some()
                {
                    return Err(
                        "viewer supports opaque/blended, untextured base-color materials only"
                            .into(),
                    );
                }
            }
        }
    }
    Ok(())
}

fn base64(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        result.push(CHARS[(n >> 18) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            CHARS[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

const HTML: &str = r##"<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'">
<title>Tokito component inspection prototype</title>
<style>
*{box-sizing:border-box}
html,body{width:100%;height:100%;overflow:hidden}
body{margin:0;background:#111820;color:#e9f1f7;font:15px system-ui}
main{display:grid;grid-template-columns:310px minmax(0,1fr);grid-template-rows:minmax(0,1fr);height:100vh;min-height:0;overflow:hidden}
aside{min-width:0;min-height:0;padding:18px;overflow:auto;border-right:1px solid #334155}
h1{font-size:20px;margin:0 0 8px}
p{line-height:1.45}
input{width:100%;padding:10px;background:#172632;color:white;border:1px solid #486176;border-radius:6px}
button{cursor:pointer;font:inherit}
#list button{display:block;width:100%;text-align:left;margin:5px 0;padding:9px;background:#20303e;border:1px solid #3b5266;color:white;border-radius:6px}
#list button.active{background:#745a18;border-color:#ffd166}
#list button.missing{color:#c4a5a5}
#stage{position:relative;min-width:0;min-height:0;overflow:hidden}
#view{width:100%;height:100%;display:block;touch-action:none}
#selection{position:absolute;inset:0;width:100%;height:100%;display:block;pointer-events:none;z-index:2}
#help{z-index:3;position:absolute;bottom:14px;left:16px;background:#111c;padding:9px;pointer-events:none}
#status{color:#b9c9d6;font-size:13px}
#reset{padding:8px;margin-top:8px}
#error{color:#ffb8ad;white-space:pre-wrap}
@media(max-width:650px){main{grid-template-columns:220px minmax(0,1fr)}aside{padding:10px}}
</style>
<main><aside><h1>Tokito component inspector</h1><div id="board"></div><p>Reference candidate · physical fit and AR registration unverified. This view displays source geometry, not live positional guidance.</p><p>Source descriptions and footprint names are design-record claims; they do not verify a photographed specimen. Component coverage is unverified. Unlisted parts may be absent.</p><p>Approximate studio lighting uses authored colour, roughness and metallic factors. Physical materials remain unverified. Transparent groups are depth-sorted. Intersecting transparent surfaces can have visual ordering limits.</p><input id="search" aria-label="Search reference, class claim, source description or footprint claim" placeholder="Find a reference, source description or footprint"><button id="reset">Reset view / clear selection</button><p id="status" aria-live="polite"></p><pre id="error" role="alert"></pre><div id="list"></div></aside><section id="stage"><canvas id="view" aria-label="3D board; drag to orbit and scroll to zoom"></canvas><canvas id="selection" aria-hidden="true"></canvas><div id="help">Drag: orbit · Wheel: zoom · Click: select component</div></section></main>
<script id="index" type="application/json">__TOKITO_INDEX__</script>
<script id="glb" type="application/octet-stream">__TOKITO_GLB__</script>
<script>
'use strict';
(()=>{try{
const index=JSON.parse(document.getElementById('index').textContent), canvas=document.getElementById('view'),status=document.getElementById('status'),selectionCanvas=document.getElementById('selection'),overlay=selectionCanvas.getContext('2d');
document.getElementById('board').textContent=String(index.boardIdentity)+' · '+String(index.boardRevision);
if(index.referencePopulationComparison){
const audit=index.referencePopulationComparison,section=document.createElement('section');section.id='population-audit';section.setAttribute('aria-label','Reference BOM population comparison');
const note=document.createElement('p');note.textContent='Incomplete population: reference BOM comparison only. This source design is not verified as the photographed board. Counts cannot establish matching identities or positions.';section.append(note);
const table=document.createElement('table');table.style.width='100%';
const header=document.createElement('tr');for(const title of ['Part class','Reference BOM','Rendered']){const cell=document.createElement('th');cell.textContent=title;header.append(cell);}table.append(header);
for(const row of audit.classCoverageComparison){const tr=document.createElement('tr');tr.dataset.partClass=row.classClaim;for(const value of [row.classClaim,row.referenceBOMCount,row.renderedCandidateCount]){const cell=document.createElement('td');cell.textContent=String(value);tr.append(cell);}table.append(tr);}section.append(table);
const conflicts=document.createElement('p');conflicts.textContent='Conflicting reference labels: '+audit.sameLabelClassConflicts.map(c=>c.reference).join(', ')+'. No automatic identity joins were made.';section.append(conflicts);
document.getElementById('board').after(section);
}
const raw=atob(document.getElementById('glb').textContent), bytes=Uint8Array.from(raw,c=>c.charCodeAt(0)),dv=new DataView(bytes.buffer);let offset=12,doc,bin;
while(offset<bytes.length){const len=dv.getUint32(offset,true),kind=dv.getUint32(offset+4,true);if(kind===0x4e4f534a)doc=JSON.parse(new TextDecoder().decode(bytes.subarray(offset+8,offset+8+len)));if(kind===0x004e4942)bin=new DataView(bytes.buffer,offset+8,len);offset+=8+len;}
const gl=canvas.getContext('webgl2',{antialias:true,alpha:false});if(!gl)throw Error('WebGL2 is unavailable. Use a browser with WebGL2 enabled.');
function runtimeError(message){overlay.clearRect(0,0,selectionCanvas.width,selectionCanvas.height);canvas.dataset.rendered='false';document.getElementById('error').textContent='Viewer refused: '+message;throw Error(message);}
canvas.addEventListener('webglcontextlost',e=>{e.preventDefault();overlay.clearRect(0,0,selectionCanvas.width,selectionCanvas.height);canvas.dataset.rendered='false';document.getElementById('error').textContent='Viewer stopped: graphics context lost. Reload this file to inspect again.';});
const identity=()=>[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
function mul(a,b){const o=new Array(16).fill(0);for(let c=0;c<4;c++)for(let r=0;r<4;r++)for(let k=0;k<4;k++)o[c*4+r]+=a[k*4+r]*b[c*4+k];return o;}
function local(n){if(n.matrix)return n.matrix;const t=n.translation||[0,0,0],s=n.scale||[1,1,1],q=n.rotation||[0,0,0,1],[x,y,z,w]=q;return[(1-2*y*y-2*z*z)*s[0],(2*x*y+2*z*w)*s[0],(2*x*z-2*y*w)*s[0],0,(2*x*y-2*z*w)*s[1],(1-2*x*x-2*z*z)*s[1],(2*y*z+2*x*w)*s[1],0,(2*x*z+2*y*w)*s[2],(2*y*z-2*x*w)*s[2],(1-2*x*x-2*y*y)*s[2],0,...t,1];}
function point(m,p){return[m[0]*p[0]+m[4]*p[1]+m[8]*p[2]+m[12],m[1]*p[0]+m[5]*p[1]+m[9]*p[2]+m[13],m[2]*p[0]+m[6]*p[1]+m[10]*p[2]+m[14]];}
const componentBounds=new Map();function includeBounds(id,world,minimum,maximum){if(id<0)return;let bounds=componentBounds.get(id);if(!bounds){bounds={minimum:[Infinity,Infinity,Infinity],maximum:[-Infinity,-Infinity,-Infinity]};componentBounds.set(id,bounds);}for(let mask=0;mask<8;mask++){const corner=point(world,[mask&1?maximum[0]:minimum[0],mask&2?maximum[1]:minimum[1],mask&4?maximum[2]:minimum[2]]);for(let axis=0;axis<3;axis++){bounds.minimum[axis]=Math.min(bounds.minimum[axis],corner[axis]);bounds.maximum[axis]=Math.max(bounds.maximum[axis],corner[axis]);}}}
function normalMatrix(m){const a=m[0],b=m[4],c=m[8],d=m[1],e=m[5],f=m[9],g=m[2],h=m[6],i=m[10],det=a*(e*i-f*h)-b*(d*i-f*g)+c*(d*h-e*g);if(Math.abs(det)<1e-20)throw Error('Singular node transform cannot be rendered reliably');return[(e*i-f*h)/det,(c*h-b*i)/det,(b*f-c*e)/det,(f*g-d*i)/det,(a*i-c*g)/det,(c*d-a*f)/det,(d*h-e*g)/det,(b*g-a*h)/det,(a*e-b*d)/det];}
function readAccessor(i){const a=doc.accessors[i],v=doc.bufferViews[a.bufferView],size=a.type==='VEC4'?4:a.type==='VEC3'?3:1,width={5121:1,5123:2,5125:4,5126:4}[a.componentType],stride=v.byteStride||size*width,start=(v.byteOffset||0)+(a.byteOffset||0),out=a.componentType===5126?new Float32Array(a.count*size):new Uint32Array(a.count*size);for(let n=0;n<a.count;n++)for(let k=0;k<size;k++){const o=start+n*stride+k*width;out[n*size+k]=a.componentType===5126?bin.getFloat32(o,true):a.componentType===5121?bin.getUint8(o):a.componentType===5123?bin.getUint16(o,true):bin.getUint32(o,true);}return out;}
function shader(type,source){const s=gl.createShader(type);gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;}
const program=gl.createProgram();gl.attachShader(program,shader(gl.VERTEX_SHADER,`#version 300 es
in vec3 position;in vec3 normal;in vec4 vertexColor;out vec4 vc;uniform mat4 world;uniform mat4 vp;uniform mat3 nm;out vec3 p;out vec3 n;void main(){vec4 w=world*vec4(position,1.);p=w.xyz;n=nm*normal;vc=vertexColor;gl_Position=vp*w;gl_PointSize=4.;}`));gl.attachShader(program,shader(gl.FRAGMENT_SHADER,`#version 300 es
precision highp float;in vec4 vc;in vec3 p;in vec3 n;uniform vec3 color;uniform vec3 emission;uniform vec3 eye;uniform float alpha;uniform float roughness;uniform float metallic;uniform bool picking;uniform bool hasNormal;uniform bool surface;out vec4 outColor;
vec3 light(vec3 base,vec3 norm,vec3 view,vec3 direction,vec3 radiance){vec3 halfVector=normalize(view+direction);float nl=max(dot(norm,direction),0.);float nv=max(dot(norm,view),.001);float nh=max(dot(norm,halfVector),0.);float vh=max(dot(view,halfVector),0.);float a=max(roughness*roughness,.0025);float a2=a*a;float den=nh*nh*(a2-1.)+1.;float distribution=a2/(3.14159265*den*den);float k=(roughness+1.)*(roughness+1.)/8.;float geometry=nv/(nv*(1.-k)+k)*nl/(nl*(1.-k)+k);vec3 fresnel=mix(vec3(.04),base,metallic);fresnel=fresnel+(1.-fresnel)*pow(1.-vh,5.);vec3 specular=distribution*geometry*fresnel/max(4.*nv*nl,.001);vec3 diffuse=(1.-fresnel)*(1.-metallic)*base/3.14159265;return (diffuse+specular)*radiance*nl;}
void main(){if(picking){outColor=vec4(color,1.);return;}vec3 base=color*vc.rgb;if(!surface){outColor=vec4(base,alpha*vc.a);return;}vec3 norm=hasNormal?normalize(n):normalize(cross(dFdx(p),dFdy(p)));if(!gl_FrontFacing)norm=-norm;vec3 view=normalize(eye-p);vec3 result=base*.18+emission;result+=light(base,norm,view,normalize(vec3(.4,.8,.6)),vec3(2.4));result+=light(base,norm,view,normalize(vec3(-.6,.4,-.5)),vec3(.7,.8,1.));result+=light(base,norm,view,normalize(vec3(.1,.2,-.9)),vec3(.6));outColor=vec4(pow(clamp(result,0.,1.),vec3(1./2.2)),alpha*vc.a);}`));gl.linkProgram(program);if(!gl.getProgramParameter(program,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(program));gl.useProgram(program);
const uniforms=Object.fromEntries(['world','vp','nm','color','emission','alpha','picking','hasNormal','surface','eye','roughness','metallic'].map(k=>[k,gl.getUniformLocation(program,k)])),attrs={p:gl.getAttribLocation(program,'position'),n:gl.getAttribLocation(program,'normal'),c:gl.getAttribLocation(program,'vertexColor')};
const owner=new Map();index.components.forEach((c,i)=>{if(c.selectorNodeIndex!==null&&c.selectorNodeIndex!==undefined)owner.set(c.selectorNodeIndex,i);});
const draws=[],seen=new Set(),lo=[Infinity,Infinity,Infinity],hi=[-Infinity,-Infinity,-Infinity];let vertexCount=0,triangleCount=0,lineSegmentCount=0,pointCount=0;
function walk(ni,parent,inherited){
if(seen.has(ni))throw Error('Shared/cyclic scene node');seen.add(ni);const node=doc.nodes[ni];if(!node)throw Error('Missing scene node');const world=mul(parent,local(node)),id=owner.has(ni)?owner.get(ni):inherited;
if(node.mesh!==undefined){const groups=new Map();let primitiveNumber=0;
for(const primitive of doc.meshes[node.mesh].primitives){const pos=readAccessor(primitive.attributes.POSITION),norm=primitive.attributes.NORMAL===undefined?null:readAccessor(primitive.attributes.NORMAL),colors=primitive.attributes.COLOR_0===undefined?null:readAccessor(primitive.attributes.COLOR_0),indices=primitive.indices===undefined?null:readAccessor(primitive.indices),mode=primitive.mode??4,key=String(primitive.material??'default')+':'+!!norm+':'+!!colors+':'+mode+((mode===2||mode===3)?':'+primitiveNumber:'');primitiveNumber++;
let group=groups.get(key);if(!group){group={positions:[],normals:norm?[]:null,colors:colors?[]:null,indices:[],material:primitive.material,mode};groups.set(key,group);}const base=group.positions.length/3;for(const v of pos)group.positions.push(v);if(norm)for(const v of norm)group.normals.push(v);if(colors)for(const v of colors)group.colors.push(v);if(indices){for(const v of indices)group.indices.push(v+base);}else{for(let v=0;v<pos.length/3;v++)group.indices.push(v+base);}
}
for(const group of groups.values()){const pos=new Float32Array(group.positions),norm=group.normals?new Float32Array(group.normals):null,indices=new Uint32Array(group.indices),vao=gl.createVertexArray();gl.bindVertexArray(vao);let buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,pos,gl.STATIC_DRAW);gl.enableVertexAttribArray(attrs.p);gl.vertexAttribPointer(attrs.p,3,gl.FLOAT,false,0,0);
if(norm){buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,norm,gl.STATIC_DRAW);gl.enableVertexAttribArray(attrs.n);gl.vertexAttribPointer(attrs.n,3,gl.FLOAT,false,0,0);}else{gl.disableVertexAttribArray(attrs.n);gl.vertexAttrib3f(attrs.n,0,0,0);}if(group.colors){buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(group.colors),gl.STATIC_DRAW);gl.enableVertexAttribArray(attrs.c);gl.vertexAttribPointer(attrs.c,4,gl.FLOAT,false,0,0);}else{gl.disableVertexAttribArray(attrs.c);gl.vertexAttrib4f(attrs.c,1,1,1,1);}buffer=gl.createBuffer();gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER,buffer);gl.bufferData(gl.ELEMENT_ARRAY_BUFFER,indices,gl.STATIC_DRAW);
const material=(doc.materials||[])[group.material],color=material?.pbrMetallicRoughness?.baseColorFactor?.slice(0,3)||[.56,.66,.72],transparent=material?.alphaMode==='BLEND',alpha=transparent?(material?.pbrMetallicRoughness?.baseColorFactor?.[3]??1):1,bmin=[Infinity,Infinity,Infinity],bmax=[-Infinity,-Infinity,-Infinity];for(let k=0;k<pos.length;k+=3)for(let j=0;j<3;j++){bmin[j]=Math.min(bmin[j],pos[k+j]);bmax[j]=Math.max(bmax[j],pos[k+j]);}includeBounds(id,world,bmin,bmax);const drawCenter=point(world,bmin.map((v,j)=>(v+bmax[j])/2));draws.push({vao,world,nm:normalMatrix(world),id,color,alpha,transparent,emission:material?.emissiveFactor||[0,0,0],roughness:material?.pbrMetallicRoughness?.roughnessFactor??1,metallic:material?.pbrMetallicRoughness?.metallicFactor??1,center:drawCenter,count:indices.length,indexed:true,normal:!!norm,mode:group.mode});vertexCount+=pos.length/3;if(group.mode===4)triangleCount+=indices.length/3;else if(group.mode===0)pointCount+=indices.length;else lineSegmentCount+=group.mode===1?indices.length/2:group.mode===2?indices.length:indices.length-1;
for(let k=0;k<pos.length;k+=3){const p=point(world,[pos[k],pos[k+1],pos[k+2]]);for(let j=0;j<3;j++){lo[j]=Math.min(lo[j],p[j]);hi[j]=Math.max(hi[j],p[j]);}}
}}
for(const child of node.children||[])walk(child,world,id);}
const roots=doc.scenes[doc.scene??0].nodes||[];for(const root of roots)walk(root,identity(),-1);if(!draws.length)throw Error('No renderable triangles in active scene');
const center=lo.map((v,i)=>(v+hi[i])/2),radius=Math.max(...lo.map((v,i)=>hi[i]-v))/2;if(!Number.isFinite(radius)||radius<=0)throw Error('Invalid scene bounds');let yaw=.65,pitch=.6,distance=radius*3.8,selected=-1;
const sub=(a,b)=>a.map((v,i)=>v-b[i]),cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]],unit=a=>{const l=Math.hypot(...a);return a.map(x=>x/l);},dot=(a,b)=>a.reduce((s,x,i)=>s+x*b[i],0);
let cameraEye,cameraZ;
function camera(){const eye=[center[0]+distance*Math.cos(pitch)*Math.sin(yaw),center[1]+distance*Math.sin(pitch),center[2]+distance*Math.cos(pitch)*Math.cos(yaw)],z=unit(sub(eye,center)),x=unit(cross([0,1,0],z)),y=cross(z,x),view=[x[0],y[0],z[0],0,x[1],y[1],z[1],0,x[2],y[2],z[2],0,-dot(x,eye),-dot(y,eye),-dot(z,eye),1],near=radius*.0005,far=distance+radius*8,f=1/Math.tan(Math.PI/8),aspect=canvas.width/canvas.height,projection=[f/aspect,0,0,0,0,f,0,0,0,0,(far+near)/(near-far),-1,0,0,2*far*near/(near-far),0];cameraEye=eye;cameraZ=z;return mul(projection,view);}
let pickBuffer=null,pickTexture=null,pickDepth=null,pickWidth=0,pickHeight=0;
function resize(){const rect=canvas.getBoundingClientRect(),ratio=Math.min(devicePixelRatio||1,2),w=Math.min(4096,Math.max(1,Math.round(rect.width*ratio))),h=Math.min(4096,Math.max(1,Math.round(rect.height*ratio)));if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h;}if(selectionCanvas.width!==w||selectionCanvas.height!==h){selectionCanvas.width=w;selectionCanvas.height=h;}if(pickWidth!==w||pickHeight!==h){if(pickBuffer)gl.deleteFramebuffer(pickBuffer);if(pickTexture)gl.deleteTexture(pickTexture);if(pickDepth)gl.deleteRenderbuffer(pickDepth);pickBuffer=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,pickBuffer);pickTexture=gl.createTexture();gl.bindTexture(gl.TEXTURE_2D,pickTexture);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,w,h,0,gl.RGBA,gl.UNSIGNED_BYTE,null);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,pickTexture,0);pickDepth=gl.createRenderbuffer();gl.bindRenderbuffer(gl.RENDERBUFFER,pickDepth);gl.renderbufferStorage(gl.RENDERBUFFER,gl.DEPTH_COMPONENT16,w,h);gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,pickDepth);if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw Error('Picking framebuffer unavailable');pickWidth=w;pickHeight=h;}}
function drawSelection(vp){overlay.clearRect(0,0,selectionCanvas.width,selectionCanvas.height);if(selected<0)return;const bounds=componentBounds.get(selected);if(!bounds)return;const corners=[];for(let mask=0;mask<8;mask++){const p=[mask&1?bounds.maximum[0]:bounds.minimum[0],mask&2?bounds.maximum[1]:bounds.minimum[1],mask&4?bounds.maximum[2]:bounds.minimum[2]],x=vp[0]*p[0]+vp[4]*p[1]+vp[8]*p[2]+vp[12],y=vp[1]*p[0]+vp[5]*p[1]+vp[9]*p[2]+vp[13],w=vp[3]*p[0]+vp[7]*p[1]+vp[11]*p[2]+vp[15];if(!Number.isFinite(w)||w<=1e-8){corners.push(null);continue;}corners.push([(x/w*.5+.5)*selectionCanvas.width,(.5-y/w*.5)*selectionCanvas.height]);}const edges=[[0,1],[0,2],[0,4],[1,3],[1,5],[2,3],[2,6],[3,7],[4,5],[4,6],[5,7],[6,7]],scale=selectionCanvas.width/Math.max(1,selectionCanvas.clientWidth);overlay.save();overlay.strokeStyle='#ffc44d';overlay.lineWidth=2*scale;overlay.shadowColor='rgba(255,190,65,.72)';overlay.shadowBlur=5*scale;overlay.beginPath();for(const [a,b] of edges){if(!corners[a]||!corners[b])continue;overlay.moveTo(...corners[a]);overlay.lineTo(...corners[b]);}overlay.stroke();overlay.restore();}
function render(picking=false){if(gl.isContextLost()){overlay.clearRect(0,0,selectionCanvas.width,selectionCanvas.height);canvas.dataset.rendered='false';return;}resize();gl.bindFramebuffer(gl.FRAMEBUFFER,picking?pickBuffer:null);gl.viewport(0,0,canvas.width,canvas.height);gl.enable(gl.DEPTH_TEST);gl.depthMask(true);gl.disable(gl.BLEND);gl.disable(gl.DITHER);gl.clearColor(picking?0:.065,picking?0:.094,picking?0:.125,1);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);const vp=camera();gl.uniformMatrix4fv(uniforms.vp,false,vp);gl.uniform1i(uniforms.picking,picking?1:0);gl.uniform3fv(uniforms.eye,cameraEye);
const visible=draws.filter(d=>d.alpha>0),ordered=picking?visible:visible.filter(d=>!d.transparent).concat(visible.filter(d=>d.transparent).sort((a,b)=>dot(sub(a.center,cameraEye),cameraZ)-dot(sub(b.center,cameraEye),cameraZ)));
for(const d of ordered){if(!picking&&d.transparent){gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);gl.depthMask(false);}else{gl.disable(gl.BLEND);gl.depthMask(true);}gl.bindVertexArray(d.vao);gl.uniformMatrix4fv(uniforms.world,false,d.world);gl.uniformMatrix3fv(uniforms.nm,false,d.nm);gl.uniform1i(uniforms.hasNormal,d.normal?1:0);gl.uniform1i(uniforms.surface,d.mode===4?1:0);gl.uniform1f(uniforms.alpha,d.alpha);gl.uniform1f(uniforms.roughness,d.roughness);gl.uniform1f(uniforms.metallic,d.metallic);const id=d.id+1,color=picking?[((id>>16)&255)/255,((id>>8)&255)/255,(id&255)/255]:d.color;gl.uniform3fv(uniforms.color,color);gl.uniform3fv(uniforms.emission,picking?[0,0,0]:d.emission);if(d.indexed)gl.drawElements(d.mode,d.count,gl.UNSIGNED_INT,0);else gl.drawArrays(d.mode,0,d.count);}gl.depthMask(true);gl.disable(gl.BLEND);gl.bindVertexArray(null);const error=gl.getError();if(error!==gl.NO_ERROR)runtimeError('WebGL rendering error '+error);if(!picking)drawSelection(vp);}
function sourceClaimText(c){const p=c.sourceDesignClaims?.properties||{};return [p.SOURCEDESCRIPTION?'source description (unverified): '+p.SOURCEDESCRIPTION:'',p.PATTERN?'footprint claim: '+p.PATTERN:'',p.SOURCELIBREFERENCE?'library claim: '+p.SOURCELIBREFERENCE:''].filter(Boolean).join(' · ');}
function sourceGeometryWarningText(c){const count=c.degenerateTrianglesOmitted;return Number.isSafeInteger(count)&&count>0?'source assembly report claims '+count.toLocaleString()+' degenerate triangles omitted; component geometry is incomplete':'';}
function componentClassText(c){if(c.componentClassClaim)return c.componentClassClaim;const h=c.componentClassHint;if(!h?.value)return'class unspecified';const basis=h.basis==='reference_designator_prefix'?'reference-prefix hint':'source-text hint';return h.value.replaceAll('_',' ')+' ('+basis+', unverified)';}
function choose(id){selected=id;const c=index.components[id],claims=c?.sourceDesignClaims;status.textContent=c?[c.reference,componentClassText(c),sourceClaimText(c),sourceGeometryWarningText(c),claims?'source design claims are not verified against the photographed specimen; source SHA-256 '+claims.sourceSha256.slice(0,12):'',c.modelAvailable?'source model selected; physical fit unverified':'no model available; no substitute generated'].filter(Boolean).join(' · '):index.components.length+' component records · '+triangleCount.toLocaleString()+' triangles · '+lineSegmentCount.toLocaleString()+' line segments · '+pointCount.toLocaleString()+' points · '+index.missingRequiredModelCount+' missing required models';document.querySelectorAll('#list button').forEach(b=>b.classList.toggle('active',Number(b.dataset.id)===id));render();}
function list(){const query=document.getElementById('search').value.toLowerCase(),root=document.getElementById('list');root.replaceChildren();index.components.forEach((c,i)=>{const sourceText=sourceClaimText(c),geometryWarning=sourceGeometryWarningText(c),classText=componentClassText(c);if(!(c.reference+' '+classText+' '+sourceText+' '+geometryWarning).toLowerCase().includes(query))return;const b=document.createElement('button');b.textContent=c.reference+' · '+classText+(sourceText?' · '+sourceText:'')+(geometryWarning?' · '+geometryWarning:'')+(c.assemblyStateClaim==='not_fitted'?' · not fitted (footprint view pending)':c.modelAvailable?'':c.needs3dBody?' · missing model':' · source reference without package body');b.dataset.id=String(i);if(!c.modelAvailable)b.classList.add('missing');b.classList.toggle('active',i===selected);b.onclick=()=>choose(i);root.append(b);});}
let drag=null;canvas.addEventListener('pointerdown',e=>{drag={x:e.clientX,y:e.clientY,startX:e.clientX,startY:e.clientY,moved:false};canvas.setPointerCapture(e.pointerId);});canvas.addEventListener('pointermove',e=>{if(!drag)return;const dx=e.clientX-drag.x,dy=e.clientY-drag.y;if(Math.hypot(e.clientX-drag.startX,e.clientY-drag.startY)>4)drag.moved=true;yaw-=dx*.007;pitch=Math.max(-1.5,Math.min(1.5,pitch+dy*.007));drag.x=e.clientX;drag.y=e.clientY;render();});canvas.addEventListener('pointerup',e=>{if(drag&&!drag.moved){const rect=canvas.getBoundingClientRect();render(true);const pixel=new Uint8Array(4),x=Math.max(0,Math.min(canvas.width-1,Math.floor((e.clientX-rect.left)*canvas.width/rect.width))),y=Math.max(0,Math.min(canvas.height-1,Math.floor((rect.bottom-e.clientY)*canvas.height/rect.height)));gl.readPixels(x,y,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixel);const id=(pixel[0]<<16|pixel[1]<<8|pixel[2])-1;choose(id>=0&&id<index.components.length?id:-1);}drag=null;});canvas.addEventListener('pointercancel',()=>{drag=null;});canvas.addEventListener('wheel',e=>{e.preventDefault();distance=Math.max(radius*.4,Math.min(radius*40,distance*Math.exp(e.deltaY*.001)));render();},{passive:false});document.getElementById('search').oninput=list;document.getElementById('reset').onclick=()=>{yaw=.65;pitch=.6;distance=radius*3.8;choose(-1);};window.addEventListener('resize',()=>render());list();choose(-1);
window.tokitoInspection={componentCount:index.components.length,triangleCount,lineSegmentCount,pointCount,vertexCount,drawCount:draws.length,snapshotImage:()=>{if(gl.isContextLost())throw Error('Graphics context lost');render();return canvas.toDataURL();},select:reference=>{const i=index.components.findIndex(c=>c.reference===reference);if(i<0)throw Error('Unknown reference');choose(i);return index.components[i];},selected:()=>selected<0?null:index.components[selected].reference};canvas.dataset.rendered='true';canvas.dataset.triangles=String(triangleCount);canvas.dataset.lines=String(lineSegmentCount);canvas.dataset.points=String(pointCount);canvas.dataset.draws=String(draws.length);
}catch(e){const layer=document.getElementById('selection');layer.getContext('2d').clearRect(0,0,layer.width,layer.height);document.getElementById('view').dataset.rendered='false';document.getElementById('error').textContent='Viewer refused: '+e.message;}})();
</script></html>"##;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::borrow::Cow;

    fn fixture() -> (Vec<u8>, Value) {
        let bin: Vec<u8> = [0f32, 0., 0., 1., 0., 0., 0., 1., 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let document = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"type":"VEC3","count":3,"min":[0.,0.,0.],"max":[1.,1.,0.]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0,"name":"ref:D1"}],"scenes":[{"nodes":[0]}],"scene":0});
        let glb = gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(serde_json::to_vec(&document).unwrap()),
            bin: Some(Cow::Owned(bin)),
        }
        .to_vec()
        .unwrap();
        let index = json!({"schema":"tokito.component-selection-index.v1","sourceSha256":{"glb":crate::sha256_hex(&glb)},"boardIdentity":"</script><script>alert(1)</script>","components":[{"reference":"D1","selectorNodeIndex":0,"modelAvailable":true,"componentClassClaim":null,"sourceDesignClaims":{"status":"unverified_source_design_claim","sourceSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","properties":{"SOURCEDESCRIPTION":"</script><script>alert(1)</script>"}}}]});
        (glb, index)
    }

    #[test]
    fn material_factors_refuse_outside_unit_interval_and_wrong_types() {
        let (glb, _) = fixture();
        let c = gltf::binary::Glb::from_slice(&glb).unwrap();
        let mut doc: Value = serde_json::from_slice(&c.json).unwrap();
        doc["meshes"][0]["primitives"][0]["material"] = json!(0);
        doc["materials"] =
            json!([{"pbrMetallicRoughness":{"roughnessFactor":0.4,"metallicFactor":0.0}}]);
        validate_geometry(&doc, c.bin.as_ref().unwrap()).unwrap();
        for field in ["roughnessFactor", "metallicFactor"] {
            for invalid in [json!(-0.1), json!(1.1), json!("0.5"), Value::Null] {
                doc["materials"][0]["pbrMetallicRoughness"][field] = invalid;
                assert!(validate_geometry(&doc, c.bin.as_ref().unwrap()).is_err());
            }
            doc["materials"][0]["pbrMetallicRoughness"][field] = json!(0.5);
        }
    }

    #[test]
    fn vertex_colours_are_bounded_and_match_positions() {
        let (glb, _) = fixture();
        let c = gltf::binary::Glb::from_slice(&glb).unwrap();
        let mut doc: Value = serde_json::from_slice(&c.json).unwrap();
        let mut bin = c.bin.unwrap().to_vec();
        bin.extend(
            [0.2f32, 0.4, 0.6, 1.0]
                .repeat(3)
                .into_iter()
                .flat_map(f32::to_le_bytes),
        );
        doc["buffers"][0]["byteLength"] = json!(bin.len());
        doc["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":36,"byteLength":48}));
        doc["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":1,"componentType":5126,"type":"VEC4","count":3}));
        doc["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"] = json!(1);
        validate_geometry(&doc, &bin).unwrap();
        doc["accessors"][1]["count"] = json!(2);
        assert!(validate_geometry(&doc, &bin).is_err());
        doc["accessors"][1]["count"] = json!(3);
        bin[36..40].copy_from_slice(&1.1f32.to_le_bytes());
        assert!(validate_geometry(&doc, &bin).is_err());
        bin[36..40].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(validate_geometry(&doc, &bin).is_err());
    }

    #[test]
    fn embeds_actual_geometry_and_safe_identity_without_network() {
        let (glb, index) = fixture();
        let html = generate(&glb, &index).unwrap();
        assert!(html.contains(&base64(&glb)));
        assert!(html.contains("\\u003c/script>"));
        assert!(!html.contains("<script>alert(1)"));
        assert!(html.contains("connect-src 'none'"));
        assert!(html.contains("gl.drawElements"));
        assert!(html.contains("tokitoInspection"));
        assert!(html.contains("source assembly report claims"));
        assert!(html.contains("component geometry is incomplete"));
    }
    #[test]
    fn refuses_wrong_binding_and_selector() {
        let (glb, mut index) = fixture();
        index["sourceSha256"]["glb"] = json!("0");
        assert!(generate(&glb, &index).unwrap_err().contains("bind"));
        index["sourceSha256"]["glb"] = json!(crate::sha256_hex(&glb));
        index["components"][0]["reference"] = json!("U2");
        assert!(generate(&glb, &index).unwrap_err().contains("selector"));
    }
    #[test]
    fn refuses_out_of_bounds_geometry_and_unsupported_modes() {
        let (glb, _) = fixture();
        let c = gltf::binary::Glb::from_slice(&glb).unwrap();
        let mut doc: Value = serde_json::from_slice(&c.json).unwrap();
        doc["accessors"][0]["count"] = json!(4);
        assert!(
            validate_geometry(&doc, c.bin.as_ref().unwrap())
                .unwrap_err()
                .contains("bounds")
        );
        doc["accessors"][0]["count"] = json!(3);
        doc["meshes"][0]["primitives"][0]["mode"] = json!(5);
        assert!(
            validate_geometry(&doc, c.bin.as_ref().unwrap())
                .unwrap_err()
                .contains("triangle")
        );
    }
    #[test]
    fn accepts_points_and_line_modes_with_correct_element_counts() {
        let (glb, _) = fixture();
        let c = gltf::binary::Glb::from_slice(&glb).unwrap();
        let mut doc: Value = serde_json::from_slice(&c.json).unwrap();
        for mode in [0, 2, 3] {
            doc["meshes"][0]["primitives"][0]["mode"] = json!(mode);
            validate_geometry(&doc, c.bin.as_ref().unwrap()).unwrap();
        }
        doc["meshes"][0]["primitives"][0]["mode"] = json!(1);
        assert!(validate_geometry(&doc, c.bin.as_ref().unwrap()).is_err());
        doc["accessors"][0]["count"] = json!(2);
        validate_geometry(&doc, c.bin.as_ref().unwrap()).unwrap();
    }
    #[test]
    fn admits_bounded_blend_alpha_but_refuses_invalid_materials() {
        let (glb, _) = fixture();
        let c = gltf::binary::Glb::from_slice(&glb).unwrap();
        let mut doc: Value = serde_json::from_slice(&c.json).unwrap();
        doc["meshes"][0]["primitives"][0]["material"] = json!(0);
        doc["materials"] = json!([{"alphaMode":"BLEND","pbrMetallicRoughness":{"baseColorFactor":[0.1,0.5,0.1,0.83]}}]);
        validate_geometry(&doc, c.bin.as_ref().unwrap()).unwrap();
        doc["materials"][0]["emissiveFactor"] = json!([0.1, 0.2, 0.3]);
        validate_geometry(&doc, c.bin.as_ref().unwrap()).unwrap();
        doc["materials"][0]["emissiveFactor"] = json!([0.1, 0.2, 1.1]);
        assert!(
            validate_geometry(&doc, c.bin.as_ref().unwrap())
                .unwrap_err()
                .contains("emissive")
        );
        doc["materials"][0]
            .as_object_mut()
            .unwrap()
            .remove("emissiveFactor");
        doc["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"][3] = json!(1.01);
        assert!(
            validate_geometry(&doc, c.bin.as_ref().unwrap())
                .unwrap_err()
                .contains("base-color")
        );
        doc["materials"][0]["pbrMetallicRoughness"]["baseColorFactor"][3] = json!(0.9);
        doc["materials"][0]["alphaMode"] = json!("MASK");
        assert!(validate_geometry(&doc, c.bin.as_ref().unwrap()).is_err());
    }
    #[test]
    fn encodes_base64_padding() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }
}
