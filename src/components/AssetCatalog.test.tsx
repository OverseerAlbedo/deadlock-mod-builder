import { fireEvent,render,screen } from '@testing-library/react';
import { describe,expect,it,vi } from 'vitest';
import { DestinationPicker,type ComponentCatalog } from './AssetCatalog';
const catalog:ComponentCatalog={hero_path:'models/heroes/test.vmdl',icons:[],animations:[],effects:[{path:'particles/hero/supported.vpcf_c',target:'particles/hero/supported.vpcf_c',label:'Shared effect',supported:true,shared:true,original_sha256:'a'.repeat(64)},{path:'particles/hero/unsupported.vpcf_c',target:'particles/hero/unsupported.vpcf_c',label:'Unsupported effect',supported:false,reason:'Unsupported renderer class'}],audio:[],variants:[],animation_unavailable:null,warnings:[]};
describe('Native destination catalog',()=>{
 it('keeps unsupported entries unselectable and returns only the chosen native record',()=>{
  const choose=vi.fn();render(<DestinationPicker kind="effects" catalog={catalog} current="" disabled={false} choose={choose} discover={()=>{}}/>);
  fireEvent.click(screen.getByRole('button',{name:'Choose game destination…'}));
  expect(screen.getByRole('button',{name:/Unsupported effect/})).toBeDisabled();
  fireEvent.click(screen.getByRole('button',{name:/Shared effect/}));expect(choose).toHaveBeenCalledWith(catalog.effects[0]);
 });
 it('limits large catalogs to a searchable window',()=>{
  const icons=Array.from({length:1500},(_,i)=>({path:`icons/${i}.vtex_c`,target:`icons/${i}.vtex_c`,label:`Icon ${i}`,supported:true}));
  render(<DestinationPicker kind="icons" catalog={{...catalog,icons}} current="" disabled={false} choose={()=>{}} discover={()=>{}}/>);fireEvent.click(screen.getByRole('button',{name:'Choose game destination…'}));
  expect(screen.getAllByRole('listitem')).toHaveLength(100);fireEvent.change(screen.getByLabelText('Search icons destinations'),{target:{value:'Icon 1499'}});expect(screen.getAllByRole('listitem')).toHaveLength(1);expect(screen.getByText('Icon 1499')).toBeInTheDocument();
 });
});
