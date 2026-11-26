import { Component, type PropsWithChildren } from 'react';
import { t } from '../i18n';
// i18n-area: preview
export class PreviewBoundary extends Component<PropsWithChildren,{error:string}>{
  state={error:''};static getDerivedStateFromError(error:unknown){return {error:error instanceof Error?error.message:String(error)};}
  render(){return this.state.error?<div role="alert" className="error-notice"><strong>{t('3D renderer unavailable')}</strong><p>{this.state.error}</p><p>{t('Model validation and building remain separate from preview rendering.')}</p></div>:this.props.children;}
}
