import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Status { installed: boolean; nodeAvailable: boolean; version: string }
export default function BrowserSetup({disabled}:{disabled:boolean}) {
  const [status,setStatus] = useState<Status|null>(null);
  const [installing,setInstalling] = useState(false);
  const [error,setError] = useState<string|null>(null);
  useEffect(()=>{let live=true;void invoke<Status>("workbench_browser_status").then(value=>{if(live)setStatus(value);}).catch(reason=>{if(live)setError(String(reason));});return()=>{live=false;};},[]);
  const install=async()=>{
    setInstalling(true);setError(null);
    try {setStatus(await invoke<Status>("workbench_browser_install"));}
    catch(reason){setError(String(reason));}
    finally{setInstalling(false);}
  };
  return <div className="rounded border border-gray-200 p-3 text-xs dark:border-gray-700">
    <div>{status?.installed ? "Browser installed. Your assistant can open it when needed." : "Install Chromium for browser interaction. This downloads the browser and uses disk space on this device."}</div>
    {status && !status.nodeAvailable && <p>Install Node.js 22 or later, then restart Pipeline to set up the browser.</p>}
    {!status?.installed && <button className="mt-2 rounded border px-2 py-1" disabled={disabled||installing||!status?.nodeAvailable} onClick={()=>void install()}>{installing?"Installing browser…":"Install browser"}</button>}
    {error && <p role="alert" className="mt-2 text-red-600">{error}</p>}
  </div>;
}
