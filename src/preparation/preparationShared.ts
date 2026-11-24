import { lazy } from 'react';
import type { JsonValue } from '../models';
import type { PreparationOptions } from './models';
const GltfViewport=lazy(()=>import('../preview/GltfViewport'));
const MappingViewport=lazy(()=>import('./MappingViewport'));
const Viewport=lazy(()=>import('../preview/Viewport'));
/** App's `run`: `keepError` keeps the failure line of the last operation (automatic preview loads). */
type Operate=(label:string,operation:()=>Promise<void>,cancel?:boolean,refreshTimings?:boolean,keepError?:boolean)=>Promise<unknown>;
const asOptions=(options:PreparationOptions)=>JSON.parse(JSON.stringify(options)) as Record<string,JsonValue>;
export { GltfViewport, MappingViewport, Viewport, asOptions };
export type { Operate };
/** What the 3D view column shows. */
export type PrepView='model'|'joints'|'result';
