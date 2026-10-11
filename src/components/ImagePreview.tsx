import { useEffect, useRef, useState } from 'react';
import { api } from '../api';
export function ImagePreview({path,disabled,fail,filter,label}:{path:string;disabled:boolean;fail:(title:string,error:unknown)=>void;filter?:string;label?:string}){
  const[url,setUrl]=useState<string|null>(null),[loading,setLoading]=useState(false);
  const mounted=useRef(true);useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
  const current=useRef(path);current.current=path;
  useEffect(()=>{setUrl(null);},[path]);
  useEffect(()=>()=>{if(url)URL.revokeObjectURL(url);},[url]);
  async function preview(){setLoading(true);try{const data=await api.previewImage(path);if(mounted.current&&current.current===path)setUrl(URL.createObjectURL(new Blob([data],{type:'image/png'})));}catch(error){if(mounted.current)fail('Image preview could not be loaded',error);}finally{if(mounted.current)setLoading(false);}}
  return <div className="image-preview"><button disabled={disabled||loading||!path} onClick={()=>void preview()}>{loading?'Loading image…':label??'Preview assigned image'}</button>{url&&<><img src={url} alt="Assigned texture preview" style={filter?{filter}:undefined}/><p className="hint">Image pixels only. The game shader can change their appearance.</p></>}</div>;
}
