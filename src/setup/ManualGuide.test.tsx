import {fireEvent,render,screen} from '@testing-library/react';
import {describe,expect,it} from 'vitest';
import {ManualGuide} from './ManualGuide';
import type {ManualPreview} from './models';

const fetch=(auth:string)=>`& 'C:\\Tools\\DepotDownloader.exe' -app 730 -depot 2347770 -manifest-only -dir C:\\DMBTools\\manifests ${auth}`;
const download=(depot:string,auth:string)=>`& 'C:\\Tools\\DepotDownloader.exe' -app 730 -depot ${depot} -filelist C:\\Plans\\filelists\\${depot}.txt ${auth}`;
const preview=(exported:boolean):ManualPreview=>({
  guide:'DOWNLOAD THE MODEL TOOLSET YOURSELF',commands:'',plan:{},plan_digest:null,expected_files:'',compatible:false,downloaded:false,
  export_dir:exported?'C:\\Plans\\manual-1':null,whole_bytes:69.7*2**30,selected_bytes:exported?4*2**30:null,
  argv:Object.fromEntries((['qr','username'] as const).map(mode=>{
    const auth=mode==='qr'?'-qr':'-username YOUR_STEAM_USERNAME -remember-password';
    const depots=exported?['2347770','2347771']:[];
    return [mode,{metadata:[],downloads:depots.map(d=>['dd','-depot',d]),metadata_command:fetch(auth),download_commands:depots.map(d=>download(d,auth))}];
  })),
});

describe('Manual download guide',()=>{
  it('explains why the download commands come after the plan',()=>{
    render(<ManualGuide preview={preview(false)}/>);
    expect(screen.getByLabelText('Command to get the file lists')).toHaveTextContent('-qr');
    expect(screen.getByText('Your download commands appear here after step 3.')).toBeInTheDocument();
    expect(screen.getByText(/instead of all 69\.7 GB/)).toBeInTheDocument();
    expect(screen.queryByLabelText('Download command 1')).not.toBeInTheDocument();
  });
  it('shows one command when one run downloads every depot',()=>{
    const single=preview(true);
    for(const commands of Object.values(single.argv)){commands.downloads=[['dd','-depot','2347770','2347771','2347779','-filelist','all-depots.txt']];commands.download_commands=commands.download_commands.slice(0,1);}
    render(<ManualGuide preview={single}/>);
    expect(screen.getByText('One command for depots 2347770, 2347771, 2347779')).toBeInTheDocument();
    expect(screen.getByText(/^Run this command and let it finish/)).toBeInTheDocument();
  });
  it('shows each download part for the chosen sign-in, with the account name filled in',()=>{
    render(<ManualGuide preview={preview(true)}/>);
    expect(screen.getByText(/Part 1 of 2 · depot 2347770/)).toBeInTheDocument();
    expect(screen.getByText(/4\.0 GB in total/)).toBeInTheDocument();
    expect(screen.getByLabelText('Download command 2')).toHaveTextContent('-depot 2347771');
    fireEvent.click(screen.getByRole('radio',{name:'Steam username'}));
    expect(screen.getByLabelText('Command to get the file lists')).toHaveTextContent('YOUR_STEAM_USERNAME');
    fireEvent.change(screen.getByLabelText('Steam account name'),{target:{value:'example_account'}});
    expect(screen.getByLabelText('Command to get the file lists')).toHaveTextContent('-username example_account -remember-password');
    expect(screen.getByLabelText('Download command 1')).toHaveTextContent('-username example_account');
    // Anything that is not an account name is not pasted into a command.
    fireEvent.change(screen.getByLabelText('Steam account name'),{target:{value:'a; rm x'}});
    expect(screen.getByLabelText('Command to get the file lists')).toHaveTextContent('YOUR_STEAM_USERNAME');
  });
});
