import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { taskClient, stateLabel, type TaskSummary } from "../lib/taskClient";
import "./TasksPage.css";
export default function WorkspaceTaskCards({ sessionId, onOpen }: { sessionId: string; onOpen: (id: string) => void }) {
  const [proposals, setProposals] = useState<{ id: string; chain: { name: string } }[]>([]);
  const [error, setError] = useState("");
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  useEffect(() => { let alive = true; let timer: ReturnType<typeof setTimeout> | null = null; const refresh = () => { void invoke<{ id: string; chain: { name: string } }[]>("task_proposals", { sessionId }).then(v => { if (alive) setProposals(v); }).catch(() => {}); void taskClient.list("session", sessionId).then(rows => { if (alive) setTasks(rows.slice(0, 4)); }).catch(() => {}); }; refresh(); const unlisten = listen("tasks:changed", () => { if (!timer) timer = setTimeout(() => { timer = null; if (alive) refresh(); }, 250); }); const turns = listen("workbench:event", (e: { payload: { kind?: string } }) => { if (e.payload.kind === "turnCompleted") refresh(); }); return () => { alive = false; void turns.then(f => f()); if (timer) clearTimeout(timer); void unlisten.then(f => f()); }; }, [sessionId]);
  return tasks.length || proposals.length || error ? <div className="workspace-task-cards" aria-label="Conversation task chains">{error && <p role="alert">{error}</p>}{proposals.map(p => <button key={p.id} className="workspace-task-card" onClick={() => { void invoke<{ id: string }>("task_prepare_proposal", { sessionId, proposalId: p.id }).then(run => onOpen(run.id)).catch(e => setError(String(e))); }}><strong>{p.chain.name}</strong><span>Review proposal →</span></button>)}{tasks.map(task => <button key={task.id} className="workspace-task-card" onClick={() => onOpen(task.id)}><strong>{task.name}</strong><span>{stateLabel(task.state)}</span><span aria-hidden="true">↗</span></button>)}</div> : null;
}
