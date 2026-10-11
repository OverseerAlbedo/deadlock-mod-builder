import { Component, type PropsWithChildren } from 'react';
export class PreviewBoundary extends Component<PropsWithChildren,{error:string}>{
  state={error:''};static getDerivedStateFromError(error:unknown){return {error:error instanceof Error?error.message:String(error)};}
  render(){return this.state.error?<div role="alert" className="error-notice"><strong>3D renderer unavailable</strong><p>{this.state.error}</p><p>Model validation and building remain separate from preview rendering.</p></div>:this.props.children;}
}
