export interface GltfMetadata {source:string;source_sha256:string;warnings:string[];up_axis:'Y';[key:string]:unknown}
export interface GltfPreview {metadata:GltfMetadata;glb:ArrayBuffer}
export function decodeGltfPreview(buffer:ArrayBuffer):GltfPreview{
  if(buffer.byteLength<36||buffer.byteLength>512*1024*1024)throw new Error('Source preview packet size is invalid.');
  const bytes=new Uint8Array(buffer),view=new DataView(buffer);
  if(String.fromCharCode(...bytes.subarray(0,4))!=='DMBG'||view.getUint32(4,true)!==1||view.getUint32(12,true)!==0)throw new Error('Unsupported source preview packet.');
  const length=view.getUint32(8,true),start=16+Math.ceil(length/4)*4;
  if(length>16*1024*1024||start+20>buffer.byteLength)throw new Error('Source preview metadata is truncated.');
  const metadata=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(16,16+length))) as GltfMetadata;
  if(typeof metadata.source!=='string'||typeof metadata.source_sha256!=='string'||metadata.up_axis!=='Y'||!Array.isArray(metadata.warnings)||metadata.warnings.some(value=>typeof value!=='string'))throw new Error('Invalid source preview metadata.');
  if(view.getUint32(start,true)!==0x46546c67||view.getUint32(start+4,true)!==2||view.getUint32(start+8,true)!==buffer.byteLength-start)throw new Error('Source preview contains an invalid GLB container.');
  return {metadata,glb:buffer.slice(start)};
}
