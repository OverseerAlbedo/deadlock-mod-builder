import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import capability from '../../src-tauri/capabilities/default.json';
import { ToolDownloads, ToolLink } from './ToolDownloads';
import { toolLinks } from './toolLinks';
const mocks=vi.hoisted(()=>({openUrl:vi.fn()}));
vi.mock('@tauri-apps/plugin-opener',()=>({openUrl:mocks.openUrl}));
beforeEach(()=>{vi.clearAllMocks();mocks.openUrl.mockResolvedValue(undefined);});
describe('Program download links',()=>{
  it('opens the named publisher page with the native default-browser command',async()=>{
    render(<ToolLink id="source2viewer"/>);
    const link=screen.getByRole('link',{name:/Get Source 2 Viewer CLI/});
    expect(link).toHaveAttribute('href',toolLinks.source2viewer.url);fireEvent.click(link);
    await waitFor(()=>expect(mocks.openUrl).toHaveBeenCalledWith(toolLinks.source2viewer.url));
  });
  it('keeps links inactive while an operation is running',()=>{
    render(<ToolLink id="csdk" disabled/>);const link=screen.getByRole('link');
    expect(link).toHaveAttribute('aria-disabled','true');fireEvent.click(link);expect(mocks.openUrl).not.toHaveBeenCalled();
  });
  it('shows a usable address when the native browser launch fails',async()=>{
    mocks.openUrl.mockRejectedValue(new Error('No browser'));render(<ToolLink id="blender"/>);
    fireEvent.click(screen.getByRole('link'));expect(await screen.findByRole('alert')).toHaveTextContent(toolLinks.blender.url);
    expect(screen.getByRole('button',{name:'Copy link'})).toBeInTheDocument();
  });
  it('explains downloads and scopes browser permission to the listed pages',()=>{
    const modelSetup=vi.fn();render(<ToolDownloads busy={false} expanded modelSetup={modelSetup}/>);
    expect(screen.getByText(/Windows x64 command-line download/)).toBeInTheDocument();
    expect(toolLinks.blender.url).toBe('https://www.blender.org/releases/5-2/');
    fireEvent.click(screen.getByRole('button',{name:'Set up model compiler…'}));expect(modelSetup).toHaveBeenCalledOnce();
    const permission=capability.permissions.find(p=>typeof p==='object'&&p.identifier==='opener:allow-open-url') as {allow:{url:string}[]};
    expect(permission.allow.map(p=>p.url).sort()).toEqual(Object.values(toolLinks).map(p=>p.url).sort());
    expect(capability.permissions).not.toContain('opener:default');
  });
});
