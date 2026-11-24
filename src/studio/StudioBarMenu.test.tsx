import { useState } from 'react';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { StudioTopBar } from './Studio';
import { StudioBarMenuButton, StudioBarMenuPanel, useStudioBarMenus } from './StudioBarMenu';
import { StudioChipPanel, StudioChips, StudioOptionCards } from './StudioChoices';

function Bar(){
  const menus=useStudioBarMenus<'file'|'notes'>('t');
  const content={file:<label>Path<input/></label>,notes:<p>Two notes</p>};
  const titles={file:'File',notes:'Notes'};
  return <StudioTopBar title="model.fbx" badge="Checked" badgeTone="ok" below={menus.open?<StudioBarMenuPanel base="t" id={menus.open} title={titles[menus.open]} onClose={()=>menus.close(menus.open!)}>{content[menus.open]}</StudioBarMenuPanel>:undefined}>
    <StudioBarMenuButton base="t" id="file" open={menus.open==='file'} onToggle={()=>menus.toggle('file')} label="File"/>
    <StudioBarMenuButton base="t" id="notes" open={menus.open==='notes'} onToggle={()=>menus.toggle('notes')} label="Notes" detail="(2)" tone="attention"/>
  </StudioTopBar>;
}

describe('Top-bar menus with panels under the bar',()=>{
  it('opens one panel at a time, ties it to its button, keeps it open on outside clicks and closes it on Escape',()=>{
    render(<Bar/>);
    const file=screen.getByRole('button',{name:/File/}),notes=screen.getByRole('button',{name:/Notes/});
    expect(notes).toHaveAccessibleName(/needs a look/);
    expect(screen.getByText('Checked')).toHaveClass('studio-badge','ok');
    fireEvent.click(file);
    const panel=screen.getByRole('region',{name:'File'});
    expect(file).toHaveAttribute('aria-expanded','true');expect(file).toHaveAttribute('aria-controls',panel.id);
    fireEvent.mouseDown(document.body);fireEvent.change(within(panel).getByLabelText('Path'),{target:{value:'x'}});
    expect(screen.getByRole('region',{name:'File'})).toBeVisible();
    fireEvent.click(notes);
    expect(screen.queryByRole('region',{name:'File'})).toBeNull();
    expect(screen.getByText('Two notes')).toBeVisible();
    fireEvent.keyDown(screen.getByText('Two notes'),{key:'Escape'});
    expect(screen.queryByRole('region',{name:'Notes'})).toBeNull();
    expect(notes).toHaveFocus();
    fireEvent.click(file);fireEvent.click(screen.getByRole('button',{name:'Close File'}));
    expect(file).toHaveAttribute('aria-expanded','false');expect(file).toHaveFocus();
  });
});

describe('Chips and option cards',()=>{
  function Groups(){
    const [open,setOpen]=useState<'a'|'b'>('a'),[value,setValue]=useState<'x'|'y'|''>('');
    return <><StudioChips label="Groups" base="g" chips={[{id:'a',label:'First',detail:'x'},{id:'b',label:'Second',tone:'attention',detail:'Choose'}]} active={open} onSelect={setOpen}/>
      <StudioChipPanel base="g" id={open}>{open==='a'?<StudioOptionCards label="Pick" name="pick" options={[{value:'x',title:'Ex',badge:'default',help:'The first.'},{value:'y',title:'Why'}]} value={value} onChange={setValue}/>:<p>Second group</p>}</StudioChipPanel></>;
  }
  it('opens one group at a time with the keyboard and keeps a radio group with no choice unchecked',()=>{
    render(<Groups/>);
    const group=screen.getByRole('radiogroup',{name:'Pick'});
    expect(within(group).getAllByRole('radio').every(radio=>!(radio as HTMLInputElement).checked)).toBe(true);
    fireEvent.click(within(group).getByRole('radio',{name:/Why/}));
    expect(within(group).getByRole('radio',{name:/Why/})).toBeChecked();
    expect(screen.getByRole('tabpanel')).toHaveAttribute('aria-labelledby',screen.getByRole('tab',{name:/First/}).id);
    fireEvent.keyDown(screen.getByRole('tab',{name:/First/}),{key:'ArrowRight'});
    expect(screen.getByRole('tab',{name:/Second/})).toHaveAttribute('aria-selected','true');
    expect(screen.getByRole('tab',{name:/Second/})).toHaveFocus();
    expect(screen.getByText('Second group')).toBeVisible();
    expect(screen.queryByRole('radiogroup')).toBeNull();
  });
});
