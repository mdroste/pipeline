export default function RecoveredManuscriptDraft({
  text,
  onUse,
  onKeep,
}: {
  text: string;
  onUse: () => void;
  onKeep: () => void;
}) {
  return (
    <details className="p-3 text-sm">
      <summary>Recover an earlier manuscript draft</summary>
      <p>
        Two older editors retained different text. The shared draft is open;
        this earlier copy is preserved below.
      </p>
      <textarea
        aria-label="Earlier manuscript draft"
        className="ui-input w-full"
        readOnly
        value={text}
      />
      <button className="ui-button" onClick={onUse}>
        Use this text as the shared draft
      </button>
      <button className="ui-button" onClick={onKeep}>
        Keep the shared draft
      </button>
    </details>
  );
}
