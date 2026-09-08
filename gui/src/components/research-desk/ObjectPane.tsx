import { useEffect, useState } from 'react';
import { deskClient, type OpenResearchObject, type ResearchObject } from '../../lib/deskClient';
import { workbenchErrorMessage } from '../../lib/workbenchError';
import ReportViewer from '../ReportViewer';
import { addContextObject } from '../WorkspaceContextTray';
import { button, muted } from './shared';
export default function ObjectPane({workspaceId,object,onClose,onPin,onError}:{workspaceId:string;object:OpenResearchObject;onClose:()=>void;onPin?:()=>void;onError:(s:string)=>void}){
 const [source,setSource]=useState<ResearchObject|null>(null);
 useEffect(()=>{let alive=true;setSource(null);void deskClient.read(workspaceId,object).then(s=>{if(alive)setSource(s);}).catch(e=>{if(alive)onError(workbenchErrorMessage(e));});return()=>{alive=false;};},[workspaceId,object,onError]);
 let record:Record<string,unknown>|null=null;try{record=source?JSON.parse(source.text) as Record<string,unknown>:null;}catch{/* Plain text is a supported object body. */}
 return <section aria-label="Research object" className="flex min-h-0 min-w-0 flex-1 flex-col rounded border bg-white dark:bg-neutral-950"><header className="flex flex-wrap items-center gap-2 border-b p-3"><h2 className="min-w-0 flex-1 truncate font-semibold">{source?.title??'Opening source…'}</h2><button className={button} onClick={()=>addContextObject(workspaceId,object)}>Add to conversation</button>{onPin&&<button className={button} onClick={onPin}>Pin comparison</button>}<button className={button} onClick={onClose}>Close</button></header><div className="min-h-0 flex-1 overflow-auto p-4"><p className={muted}>{object.kind} · {source?.provenance} · {source?.access} · {source?.completeness}</p><p className={`${muted} mb-4 break-all`}>Version {object.revision}</p>{record&&typeof record==='object'?<dl className="space-y-4">{Object.entries(record).map(([key,value])=><div key={key}><dt className="text-xs font-semibold">{key.replace(/([A-Z])/g,' $1')}</dt><dd className="mt-1 whitespace-pre-wrap break-words text-sm">{typeof value==='string'?<ReportViewer markdown={value}/>:<pre className="whitespace-pre-wrap text-xs">{JSON.stringify(value,null,2)}</pre>}</dd></div>)}</dl>:<ReportViewer markdown={source?.text??''}/>} {source?.truncated&&<p className="mt-4 text-xs text-amber-700">Only part of this record is shown.</p>}</div></section>;
}
