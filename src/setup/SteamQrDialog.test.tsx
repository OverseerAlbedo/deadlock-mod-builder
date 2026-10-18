import {act,fireEvent,render,screen} from '@testing-library/react';
import {describe,expect,it,vi} from 'vitest';
const mocks=vi.hoisted(()=>({receive:null as null|((event:{payload:unknown})=>void),unlisten:vi.fn()}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(_name:string,receive:(event:{payload:unknown})=>void)=>{mocks.receive=receive;return mocks.unlisten;})}));
import {SteamQrDialog,qrPath} from './SteamQrDialog';

const rows=['1110000','1010000','1110000','0000000','0000000','0000000','0000001'];
describe('Steam QR sign-in dialog',()=>{
  it('shows the code DepotDownloader waits on and closes when the sign-in is over',async()=>{
    const {unmount}=render(<SteamQrDialog/>);
    await vi.waitFor(()=>expect(mocks.receive).not.toBeNull());
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    act(()=>mocks.receive!({payload:{rows}}));
    expect(screen.getByRole('dialog',{name:'Scan to sign in to Steam'})).toBeInTheDocument();
    const svg=screen.getByRole('img',{name:'Steam sign-in QR code'});
    // Seven modules plus a quiet zone of four on each side.
    expect(svg.getAttribute('viewBox')).toBe('0 0 15 15');
    expect(svg.querySelector('path')?.getAttribute('d')).toBe(qrPath(rows));
    // Hidden by the user until the next sign-in; a new sign-in shows again.
    fireEvent.click(screen.getByRole('button',{name:'Hide until the next sign-in'}));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    act(()=>mocks.receive!({payload:{rows}}));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    act(()=>mocks.receive!({payload:null}));
    act(()=>mocks.receive!({payload:{rows}}));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    act(()=>mocks.receive!({payload:null}));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    unmount();expect(mocks.unlisten).toHaveBeenCalled();
  });
  it('draws one square per dark module, offset by the quiet zone',()=>{
    expect(qrPath(['10','01'])).toBe('M4 4h1v1h-1zM5 5h1v1h-1z');
  });
});
