export interface Slice {offset:number;count:number}
export interface PreviewMesh {
  name:string;source_name:string;material:string;vertex_count:number;index_count:number;
  positions:Slice;normals:Slice;uvs:Slice|null;colors:Slice|null;indices:Slice;
  skin?:{influences:number;joint_names:string[];width:number;height:number;data:Slice}|null;
}
export interface PreviewMetadata {
  schema_version:1;source:string;source_sha256:string;up_axis:'Z';warnings:string[];
  bounds:{min:[number,number,number];max:[number,number,number]};meshes:PreviewMesh[];
  bones:{name:string;parent:number;position:[number,number,number]}[];
}
export interface PreviewScene {metadata:PreviewMetadata;buffer:ArrayBuffer;dataOffset:number}
export function decodePreview(buffer:ArrayBuffer):PreviewScene {
  if(!(buffer instanceof ArrayBuffer)||buffer.byteLength<16||buffer.byteLength>512*1024*1024)throw new Error('Invalid preview packet size.');
  const header=new DataView(buffer);if(header.getUint32(0,true)!==0x50424d44||header.getUint32(4,true)!==1)throw new Error('Unsupported preview packet format.');
  const jsonLength=header.getUint32(8,true),dataOffset=16+Math.ceil(jsonLength/4)*4;
  if(jsonLength>16*1024*1024||dataOffset>buffer.byteLength)throw new Error('Truncated preview metadata.');
  const metadata=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(new Uint8Array(buffer,16,jsonLength))) as PreviewMetadata;
  const finiteVector=(value:unknown):value is [number,number,number]=>Array.isArray(value)&&value.length===3&&value.every(x=>typeof x==='number'&&Number.isFinite(x));
  if(metadata.schema_version!==1||metadata.up_axis!=='Z'||typeof metadata.source!=='string'||!Array.isArray(metadata.meshes)||!Array.isArray(metadata.bones)||metadata.bones.length>16384||!metadata.bounds||!finiteVector(metadata.bounds.min)||!finiteVector(metadata.bounds.max)||!Array.isArray(metadata.warnings)||metadata.warnings.some(value=>typeof value!=='string'))throw new Error('Invalid preview metadata.');
  const checkSlice=(slice:Slice|null,expected:number)=>{if(!slice||!Number.isSafeInteger(slice.offset)||!Number.isSafeInteger(slice.count)||slice.offset<0||slice.offset%4||slice.count!==expected||dataOffset+slice.offset+slice.count*4>buffer.byteLength)throw new Error('Invalid preview vertex buffer.');};
  let vertices=0,triangles=0;
  for(const mesh of metadata.meshes){if(typeof mesh.name!=='string'||typeof mesh.material!=='string'||!Number.isSafeInteger(mesh.vertex_count)||mesh.vertex_count<0||!Number.isSafeInteger(mesh.index_count)||mesh.index_count<0||mesh.index_count%3)throw new Error('Invalid preview mesh.');vertices+=mesh.vertex_count;triangles+=mesh.index_count/3;if(vertices>6000000||triangles>4000000)throw new Error('Preview exceeds the supported geometry limits.');checkSlice(mesh.positions,mesh.vertex_count*3);checkSlice(mesh.normals,mesh.vertex_count*3);checkSlice(mesh.indices,mesh.index_count);if(mesh.uvs)checkSlice(mesh.uvs,mesh.vertex_count*2);if(mesh.colors)checkSlice(mesh.colors,mesh.vertex_count*4);if(mesh.skin){const skin=mesh.skin;if(!Number.isInteger(skin.influences)||skin.influences<1||skin.influences>32||!Number.isInteger(skin.width)||!Number.isInteger(skin.height)||skin.width<1||skin.width>4096||skin.height<1||!Array.isArray(skin.joint_names)||skin.joint_names.length>16384||skin.joint_names.some(name=>typeof name!=='string')||skin.width*skin.height*4<mesh.vertex_count*skin.influences*2)throw new Error('Invalid preview weight buffers.');checkSlice(skin.data,skin.width*skin.height*4);}}
  for(const bone of metadata.bones){if(typeof bone.name!=='string'||!Number.isInteger(bone.parent)||bone.parent< -1||bone.parent>=metadata.bones.length||!finiteVector(bone.position))throw new Error('Invalid preview skeleton.');}
  return {metadata,buffer,dataOffset};
}
export const floatBuffer=(scene:PreviewScene,slice:Slice)=>new Float32Array(scene.buffer,scene.dataOffset+slice.offset,slice.count);
export const indexBuffer=(scene:PreviewScene,slice:Slice)=>new Uint32Array(scene.buffer,scene.dataOffset+slice.offset,slice.count);
