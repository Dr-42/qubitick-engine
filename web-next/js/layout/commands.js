/**
 * commands.js — pure layout command reducers (M1.22 / D-032 / D-033).
 *
 * Part A — pane + tree commands.
 * Part B — root / anchor / hud / modal / camera / layout.replace commands.
 */

import { normalize } from './serialize.js';
import { validate } from './validate.js';
import { getAt, paneIds, findPane, removePane, insertBeside, insertTab, clone } from './tree-ops.js';
import { resolveAnchors, detectCycle, referencePoint } from './anchors.js';

const CMD_SET = new Set([
  'pane.add','pane.remove','pane.setParams',
  'tree.insert','tree.move','tree.remove','tree.sizes','tree.active','tree.cycle',
  'root.add','root.move','root.resize','root.remove',
  'anchor.set','anchor.offset','anchor.clear',
  'hud.set','hud.remove',
  'modal.set','modal.remove',
  'camera.set','layout.replace',
]);

function isPlainObject(v){return v!==null&&typeof v==='object'&&!Array.isArray(v);}
function findRootIndex(roots,id){for(let i=0;i<roots.length;i++)if(roots[i]&&roots[i].id===id)return i;return -1;}
function findRoot(roots,id){const i=findRootIndex(roots,id);return i>=0?roots[i]:null;}
function allPlacedPaneIds(layout){const ids=new Set();const c=(t)=>{if(!t)return;for(const id of paneIds(t))ids.add(id);};for(const r of layout.roots||[])c(r&&r.tree);for(const h of layout.hud||[])c(h&&h.tree);for(const m of layout.modals||[])c(m&&m.tree);return ids;}
function finalize(layout,keepUnplaced=false){const n=normalize(layout,{keepUnplaced});const v=validate(n);if(!v.ok)return{ok:false,error:v.errors[0].message};return{ok:true,layout:n};}

function transformContainer(layout,paneId,transform){
  const spaces=[layout.roots||[],layout.hud||[],layout.modals||[]];
  for(const cont of spaces){
    for(let i=0;i<cont.length;i++){
      const t=cont[i];if(!t||!t.tree)continue;if(!findPane(t.tree,paneId))continue;
      const nt=transform(t.tree);if(!nt)return null;
      const c=clone(layout);
      if(cont===layout.roots)c.roots[i]={...c.roots[i],tree:nt};
      else if(cont===layout.hud)c.hud[i]={...c.hud[i],tree:nt};
      else c.modals[i]={...c.modals[i],tree:nt};
      return c;
    }
  }
  return null;
}

function setContainerTree(layout,container,tree){
  const{space,id}=container;const c=clone(layout);
  if(space==='root'){const ri=findRootIndex(c.roots,id);if(ri<0)return null;c.roots[ri]={...c.roots[ri],tree};return c;}
  if(space==='hud'){const hi=c.hud.findIndex(h=>h&&h.id===id);if(hi<0)return null;c.hud[hi]={...c.hud[hi],tree};return c;}
  if(space==='modal'){const mi=c.modals.findIndex(m=>m&&m.id===id);if(mi<0)return null;c.modals[mi]={...c.modals[mi],tree};return c;}
  return null;
}

function removePaneFromLayout(layout,paneId){
  for(let ri=0;ri<(layout.roots||[]).length;ri++){
    const r=layout.roots[ri];if(!r||!r.tree)continue;if(!findPane(r.tree,paneId))continue;
    const res=removePane(r.tree,paneId);
    if(res===null){const c=clone(layout);c.roots.splice(ri,1);return c;}
    if(res!==r.tree){const c=clone(layout);c.roots[ri]={...c.roots[ri],tree:res};return c;}
  }
  for(let hi=0;hi<(layout.hud||[]).length;hi++){
    const h=layout.hud[hi];if(!h||!h.tree)continue;if(!findPane(h.tree,paneId))continue;
    const res=removePane(h.tree,paneId);
    if(res===null){const c=clone(layout);c.hud.splice(hi,1);return c;}
    if(res!==h.tree){const c=clone(layout);c.hud[hi]={...c.hud[hi],tree:res};return c;}
  }
  for(let mi=0;mi<(layout.modals||[]).length;mi++){
    const m=layout.modals[mi];if(!m||!m.tree)continue;if(!findPane(m.tree,paneId))continue;
    const res=removePane(m.tree,paneId);
    if(res===null){const c=clone(layout);c.modals.splice(mi,1);return c;}
    if(res!==m.tree){const c=clone(layout);c.modals[mi]={...c.modals[mi],tree:res};return c;}
  }
  return null;
}

function cmdPaneAdd(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'pane.add: id required'};
  if(!cmd.kind||typeof cmd.kind!=='string')return{ok:false,error:'pane.add: kind required'};
  const c=clone(layout);if(!c.panes)c.panes={};
  if(cmd.id in c.panes)return{ok:false,error:'pane '+cmd.id+' already defined'};
  c.panes[cmd.id]={kind:cmd.kind,params:cmd.params||{}};
  return finalize(c,true);
}
function cmdPaneRemove(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'pane.remove: id required'};
  if(allPlacedPaneIds(layout).has(cmd.id))return{ok:false,error:'pane '+cmd.id+' is still placed; remove it from all roots/hud/modals first'};
  const c=clone(layout);if(!c.panes||!(cmd.id in c.panes))return{ok:false,error:'pane '+cmd.id+' not defined'};
  delete c.panes[cmd.id];return finalize(c);
}
function cmdPaneSetParams(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'pane.setParams: id required'};
  if(!isPlainObject(cmd.params))return{ok:false,error:'pane.setParams: params must be an object'};
  const c=clone(layout);if(!c.panes||!(cmd.id in c.panes))return{ok:false,error:'pane '+cmd.id+' not defined'};
  c.panes[cmd.id].params=cmd.params;return finalize(c);
}

function cmdTreeInsert(layout,cmd){
  if(!cmd.pane||typeof cmd.pane!=='string')return{ok:false,error:'tree.insert: pane required'};
  if(!layout.panes||!(cmd.pane in layout.panes))return{ok:false,error:'pane '+cmd.pane+' not defined'};
  if(allPlacedPaneIds(layout).has(cmd.pane))return{ok:false,error:'pane already placed'};
  const to=cmd.to;if(!to)return{ok:false,error:'tree.insert: to required'};
  if(to.beside){
    if(!to.side||!['left','right','top','bottom'].includes(to.side))return{ok:false,error:'tree.insert beside: side required'};
    const r=transformContainer(layout,to.beside,t=>insertBeside(t,to.beside,cmd.pane,to.side,to.ratio===undefined?0.5:to.ratio));
    if(!r)return{ok:false,error:'pane '+to.beside+' not placed'};return finalize(r);
  }
  if(to.tabInto){
    const r=transformContainer(layout,to.tabInto,t=>insertTab(t,to.tabInto,cmd.pane));
    if(!r)return{ok:false,error:'pane '+to.tabInto+' not placed or not insertable as tab'};return finalize(r);
  }
  if(to.newRoot){
    const nr=to.newRoot;
    if(!isPlainObject(nr)||typeof nr.x!=='number'||typeof nr.y!=='number')return{ok:false,error:'tree.insert newRoot: x,y required'};
    const rid=to.newRoot.id||cmd.pane;
    if(findRootIndex(layout.roots,rid)>=0)return{ok:false,error:'root '+rid+' already exists'};
    const c=clone(layout);c.roots.push({id:rid,x:nr.x,y:nr.y,w:nr.w||300,h:nr.h||200,tree:{pane:cmd.pane}});
    return finalize(c);
  }
  if(to.container){
    const{space,id}=to.container;const c=clone(layout);
    if(space==='root'){const ri=findRootIndex(c.roots,id);if(ri<0)return{ok:false,error:'root '+id+' not found'};c.roots[ri]={...c.roots[ri],tree:{pane:cmd.pane}};return finalize(c);}
    if(space==='hud'){const hi=c.hud.findIndex(h=>h&&h.id===id);if(hi<0)return{ok:false,error:'hud '+id+' not found'};c.hud[hi]={...c.hud[hi],tree:{pane:cmd.pane}};return finalize(c);}
    if(space==='modal'){const mi=c.modals.findIndex(m=>m&&m.id===id);if(mi<0)return{ok:false,error:'modal '+id+' not found'};c.modals[mi]={...c.modals[mi],tree:{pane:cmd.pane}};return finalize(c);}
    return{ok:false,error:'tree.insert container: unknown space'};
  }
  return{ok:false,error:'tree.insert: unknown to target'};
}

function cmdTreeMove(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'tree.move: id required'};
  const to=cmd.to;if(!to)return{ok:false,error:'tree.move: to required'};
  if(!allPlacedPaneIds(layout).has(cmd.id))return{ok:false,error:'pane '+cmd.id+' not placed'};
  if(to.beside===cmd.id||to.tabInto===cmd.id||(to.newRoot&&to.newRoot.id===cmd.id))return{ok:false,error:'cannot move a rect into itself'};
  const ar=removePaneFromLayout(layout,cmd.id);if(!ar)return{ok:false,error:'pane '+cmd.id+' could not be removed'};
  let r;
  if(to.beside)r=transformContainer(ar,to.beside,t=>insertBeside(t,to.beside,cmd.id,to.side,to.ratio===undefined?0.5:to.ratio));
  else if(to.tabInto)r=transformContainer(ar,to.tabInto,t=>insertTab(t,to.tabInto,cmd.id));
  else if(to.newRoot){const nr=to.newRoot;const rid=nr.id||cmd.id;if(findRootIndex(ar.roots,rid)>=0)return{ok:false,error:'root '+rid+' already exists'};ar.roots.push({id:rid,x:nr.x,y:nr.y,w:nr.w||300,h:nr.h||200,tree:{pane:cmd.id}});r=ar;}
  else if(to.container)r=setContainerTree(ar,to.container,{pane:cmd.id});
  else return{ok:false,error:'tree.move: unknown to target'};
  if(!r)return{ok:false,error:'pane '+cmd.id+' could not be inserted at destination'};
  return finalize(r);
}

function cmdTreeRemove(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'tree.remove: id required'};
  const r=removePaneFromLayout(layout,cmd.id);if(!r)return{ok:false,error:'pane '+cmd.id+' not placed'};return finalize(r);
}

function cmdTreeSizes(layout,cmd){
  if(!Array.isArray(cmd.path))return{ok:false,error:'tree.sizes: path required'};
  if(!Array.isArray(cmd.sizes))return{ok:false,error:'tree.sizes: sizes required'};
  const t=getTreeAt(layout,cmd.path);
  if(!t||(t.split!=='h'&&t.split!=='v'))return{ok:false,error:'tree.sizes: path must point to a split node'};
  for(const s of cmd.sizes)if(typeof s!=='number'||!Number.isFinite(s)||s<=0)return{ok:false,error:'tree.sizes: every size must be a finite number > 0'};
  const c=clone(layout);const n=getTreeAt(c,cmd.path);n.sizes=cmd.sizes.slice();return finalize(c);
}

function cmdTreeActive(layout,cmd){
  if(!Array.isArray(cmd.path))return{ok:false,error:'tree.active: path required'};
  if(typeof cmd.active!=='number')return{ok:false,error:'tree.active: active required'};
  const t=getTreeAt(layout,cmd.path);
  if(!t||!('tabs' in t))return{ok:false,error:'tree.active: path must point to a tabs node'};
  const c=clone(layout);const n=getTreeAt(c,cmd.path);
  n.active=Math.max(0,Math.min(cmd.active,n.tabs.length-1));return finalize(c);
}

function cmdTreeCycle(layout,cmd){
  if(!Array.isArray(cmd.path))return{ok:false,error:'tree.cycle: path required'};
  if(cmd.cycle!=='fixed'&&cmd.cycle!=='mru')return{ok:false,error:'tree.cycle: cycle must be fixed or mru'};
  const t=getTreeAt(layout,cmd.path);
  if(!t||!('tabs' in t))return{ok:false,error:'tree.cycle: path must point to a tabs node'};
  const c=clone(layout);const n=getTreeAt(c,cmd.path);n.cycle=cmd.cycle;return finalize(c);
}

function getTreeAt(layout,path){
  if(!Array.isArray(path)||path.length===0)return null;
  const f=path[0];let t;
  if(f.root!==undefined){const r=findRoot(layout.roots,f.root);if(!r)return null;t=r.tree;}
  else if(f.hud!==undefined){const h=layout.hud&&layout.hud[f.hud];if(!h)return null;t=h.tree;}
  else if(f.modal!==undefined){const m=layout.modals&&layout.modals[f.modal];if(!m)return null;t=m.tree;}
  else return null;
  if(!t)return null;
  for(let i=1;i<path.length;i++){t=getAt(t,[path[i]]);if(!t)return null;}
  return t;
}

// ---------- Part B: root / anchor / hud / modal / camera / layout.replace ----------

function cmdRootAdd(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'root.add: id required'};
  if(findRootIndex(layout.roots,cmd.id)>=0)return{ok:false,error:'root.add: root '+cmd.id+' already exists'};
  const x=typeof cmd.x==='number'?cmd.x:0;
  const y=typeof cmd.y==='number'?cmd.y:0;
  const w=typeof cmd.w==='number'&&cmd.w>0?cmd.w:300;
  const h=typeof cmd.h==='number'&&cmd.h>0?cmd.h:200;
  const c=clone(layout);
  c.roots.push({id:cmd.id,x,y,w,h,tree:cmd.tree||{pane:cmd.pane}});
  return finalize(c);
}

function cmdRootMove(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'root.move: id required'};
  if(typeof cmd.x!=='number'||typeof cmd.y!=='number')return{ok:false,error:'root.move: x,y required'};
  const c=clone(layout);const ri=findRootIndex(c.roots,cmd.id);if(ri<0)return{ok:false,error:'root.move: root '+cmd.id+' not found'};
  const anchors = Array.isArray(c.anchors) ? c.anchors : [];
  const a = anchors.find((x) => x && x.root === cmd.id);
  if (!a) {
    c.roots[ri] = {...c.roots[ri], x: cmd.x, y: cmd.y};
    return finalize(c);
  }
  const parent = findRoot(c.roots, a.to);
  if (!parent) return { ok: false, error: 'root.move: anchor parent ' + a.to + ' not found' };
  let resolved;
  try { resolved = resolveAnchors(c.roots, anchors); } catch (err) { return { ok: false, error: err && err.message ? err.message : String(err) }; }
  const ppos = resolved.get(a.to) || { x: parent.x, y: parent.y };
  const P = referencePoint({ x: ppos.x, y: ppos.y, w: parent.w, h: parent.h }, a.toEdge);
  const childRect = c.roots[ri];
  let dx = cmd.x - P.x;
  let dy = cmd.y - P.y;
  if (a.edge === 'right') dx = cmd.x + childRect.w - P.x;
  if (a.edge === 'bottom') dy = cmd.y + childRect.h - P.y;
  a.dx = dx;
  a.dy = dy;
  c.roots[ri] = { ...c.roots[ri], x: cmd.x, y: cmd.y };
  return finalize(c);
}

function cmdRootResize(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'root.resize: id required'};
  if(typeof cmd.w!=='number'||cmd.w<=0||typeof cmd.h!=='number'||cmd.h<=0)return{ok:false,error:'root.resize: w,h must be numbers > 0'};
  const c=clone(layout);const ri=findRootIndex(c.roots,cmd.id);if(ri<0)return{ok:false,error:'root.resize: root '+cmd.id+' not found'};
  c.roots[ri]={...c.roots[ri],w:cmd.w,h:cmd.h};return finalize(c);
}

function cmdRootRemove(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'root.remove: id required'};
  const c=clone(layout);const ri=findRootIndex(c.roots,cmd.id);if(ri<0)return{ok:false,error:'root.remove: root '+cmd.id+' not found'};
  c.roots.splice(ri,1);
  if (Array.isArray(c.anchors)) {
    c.anchors = c.anchors.filter((a) => a && a.root !== cmd.id && a.to !== cmd.id);
  }
  return finalize(c);
}

function anchorValid(a){
  const EDGE_SET=new Set(['left','right','top','bottom']);
  if(!isPlainObject(a))return{ok:false,error:'anchor: must be an object'};
  if(typeof a.root!=='string'||!a.root)return{ok:false,error:'anchor.set: root required'};
  if(typeof a.to!=='string'||!a.to)return{ok:false,error:'anchor.set: to required'};
  if(a.root===a.to)return{ok:false,error:'anchor.set: root and to must differ'};
  if(!EDGE_SET.has(a.edge))return{ok:false,error:'anchor.set: edge must be left/right/top/bottom'};
  if(!EDGE_SET.has(a.toEdge))return{ok:false,error:'anchor.set: toEdge must be left/right/top/bottom'};
  if(typeof a.dx!=='number'||!Number.isFinite(a.dx))return{ok:false,error:'anchor.set: dx must be a finite number'};
  if(typeof a.dy!=='number'||!Number.isFinite(a.dy))return{ok:false,error:'anchor.set: dy must be a finite number'};
  return{ok:true};
}

function cmdAnchorSet(layout,cmd){
  const av=anchorValid(cmd);if(!av.ok)return av;
  if(!findRoot(layout.roots,cmd.root))return{ok:false,error:'anchor.set: root '+cmd.root+' not found'};
  if(!findRoot(layout.roots,cmd.to))return{ok:false,error:'anchor.set: to '+cmd.to+' not found'};
  if(cmd.root===cmd.to)return{ok:false,error:'anchor.set: root and to must differ'};
  const c=clone(layout);
  if(!Array.isArray(c.anchors))c.anchors=[];
  const existing=c.anchors.findIndex(a=>a&&a.root===cmd.root);
  if(existing>=0)return{ok:false,error:'anchor.set: root '+cmd.root+' already has an anchor (at most one parent)'};
  try { detectCycle([...c.anchors, {root:cmd.root,to:cmd.to,edge:cmd.edge,toEdge:cmd.toEdge,dx:cmd.dx,dy:cmd.dy}]); }   // eslint-disable-line
  catch (err) { return { ok: false, error: 'anchor.set: ' + (err && err.message ? err.message : String(err)) }; }
  c.anchors.push({root:cmd.root,to:cmd.to,edge:cmd.edge,toEdge:cmd.toEdge,dx:cmd.dx,dy:cmd.dy});
  return finalize(c);
}

function cmdAnchorOffset(layout,cmd){
  if(!cmd.root||typeof cmd.root!=='string')return{ok:false,error:'anchor.offset: root required'};
  if(typeof cmd.dx!=='number'||!Number.isFinite(cmd.dx))return{ok:false,error:'anchor.offset: dx must be a finite number'};
  if(typeof cmd.dy!=='number'||!Number.isFinite(cmd.dy))return{ok:false,error:'anchor.offset: dy must be a finite number'};
  const c=clone(layout);const i=c.anchors.findIndex(a=>a&&a.root===cmd.root);if(i<0)return{ok:false,error:'anchor.offset: no anchor for root '+cmd.root};
  c.anchors[i]={...c.anchors[i],dx:cmd.dx,dy:cmd.dy};return finalize(c);
}

function cmdAnchorClear(layout,cmd){
  if(!cmd.root||typeof cmd.root!=='string')return{ok:false,error:'anchor.clear: root required'};
  const c=clone(layout);const i=c.anchors.findIndex(a=>a&&a.root===cmd.root);if(i<0)return{ok:false,error:'anchor.clear: no anchor for root '+cmd.root};
  c.anchors.splice(i,1);return finalize(c);
}

function cmdHudSet(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'hud.set: id required'};
  if(!['top','bottom','left','right'].includes(cmd.dock))return{ok:false,error:'hud.set: dock must be top/bottom/left/right'};
  if(typeof cmd.size!=='number'||cmd.size<=0)return{ok:false,error:'hud.set: size must be a number > 0'};
  const c=clone(layout);if(!c.hud)c.hud=[];
  const i=c.hud.findIndex(h=>h&&h.id===cmd.id);
  const entry={id:cmd.id,dock:cmd.dock,size:cmd.size,tree:cmd.tree||{pane:cmd.pane}};
  if(i>=0)c.hud[i]=entry;else c.hud.push(entry);
  return finalize(c);
}

function cmdHudRemove(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'hud.remove: id required'};
  const c=clone(layout);const i=c.hud&&c.hud.findIndex(h=>h&&h.id===cmd.id);if(i<0)return{ok:false,error:'hud.remove: hud '+cmd.id+' not found'};
  c.hud.splice(i,1);return finalize(c);
}

function cmdModalSet(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'modal.set: id required'};
  if(!Array.isArray(cmd.open)||!cmd.open.every(v=>typeof v==='string'))return{ok:false,error:'modal.set: open must be an array of strings'};
  if(!Array.isArray(cmd.close)||!cmd.close.every(v=>typeof v==='string'))return{ok:false,error:'modal.set: close must be an array of strings'};
  const c=clone(layout);if(!c.modals)c.modals=[];
  const i=c.modals.findIndex(m=>m&&m.id===cmd.id);
  const entry={id:cmd.id,open:cmd.open.slice(),close:cmd.close.slice(),tree:cmd.tree||{pane:cmd.pane}};
  if(cmd.persistent!==undefined)entry.persistent=!!cmd.persistent;
  if(i>=0)c.modals[i]=entry;else c.modals.push(entry);
  return finalize(c);
}

function cmdModalRemove(layout,cmd){
  if(!cmd.id||typeof cmd.id!=='string')return{ok:false,error:'modal.remove: id required'};
  const c=clone(layout);const i=c.modals&&c.modals.findIndex(m=>m&&m.id===cmd.id);if(i<0)return{ok:false,error:'modal.remove: modal '+cmd.id+' not found'};
  c.modals.splice(i,1);return finalize(c);
}

function cmdCameraSet(layout,cmd){
  if(!isPlainObject(cmd))return{ok:false,error:'camera.set: camera must be an object'};
  const c=clone(layout);if(!c.camera)c.camera={};
  if(typeof cmd.x==='number')c.camera.x=cmd.x;
  if(typeof cmd.y==='number')c.camera.y=cmd.y;
  if(typeof cmd.z==='number'&&cmd.z>=0.04&&cmd.z<=5)c.camera.z=cmd.z;
  if(cmd.lock==='free'||cmd.lock==='fit')c.camera.lock=cmd.lock;
  return finalize(c);
}

function cmdLayoutReplace(layout,cmd){
  if(!isPlainObject(cmd.layout))return{ok:false,error:'layout.replace: layout must be an object'};
  return finalize(clone(cmd.layout));
}

export function reduce(layout,cmd){
  if(!isPlainObject(layout))return{ok:false,error:'reduce: layout must be an object'};
  if(!cmd||!cmd.type||!CMD_SET.has(cmd.type))return{ok:false,error:'reduce: unknown or missing command type'};
  switch(cmd.type){
    case'pane.add':return cmdPaneAdd(layout,cmd);
    case'pane.remove':return cmdPaneRemove(layout,cmd);
    case'pane.setParams':return cmdPaneSetParams(layout,cmd);
    case'tree.insert':return cmdTreeInsert(layout,cmd);
    case'tree.move':return cmdTreeMove(layout,cmd);
    case'tree.remove':return cmdTreeRemove(layout,cmd);
    case'tree.sizes':return cmdTreeSizes(layout,cmd);
    case'tree.active':return cmdTreeActive(layout,cmd);
    case'tree.cycle':return cmdTreeCycle(layout,cmd);
    case'root.add':return cmdRootAdd(layout,cmd);
    case'root.move':return cmdRootMove(layout,cmd);
    case'root.resize':return cmdRootResize(layout,cmd);
    case'root.remove':return cmdRootRemove(layout,cmd);
    case'anchor.set':return cmdAnchorSet(layout,cmd);
    case'anchor.offset':return cmdAnchorOffset(layout,cmd);
    case'anchor.clear':return cmdAnchorClear(layout,cmd);
    case'hud.set':return cmdHudSet(layout,cmd);
    case'hud.remove':return cmdHudRemove(layout,cmd);
    case'modal.set':return cmdModalSet(layout,cmd);
    case'modal.remove':return cmdModalRemove(layout,cmd);
    case'camera.set':return cmdCameraSet(layout,cmd);
    case'layout.replace':return cmdLayoutReplace(layout,cmd);
    default:return{ok:false,error:'reduce: unhandled command '+cmd.type};
  }
}
