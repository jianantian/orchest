import { useI18n } from "../../i18n";

/** An AI-proposed replacement for manuscript lines `from`–`to` (1-based,
 *  closed interval — the same line convention as lib/lyrics.ts and the
 *  "第 N–M 行" UI copy). Task 10 produces these from Done.lines. */
export interface Proposal {
  from: number;
  to: number;
  original: string[];
  replacement: string[];
}

export interface LyricsProposalProps {
  proposal: Proposal;
  onAccept: () => void;
  onReject: () => void;
}

/** Inline diff hunk rendered inside the manuscript in place of the
 *  proposed-from lines: old lines struck through on a danger tint, new
 *  lines on the accent tint, with an action bar for accept/reject. Pure
 *  presentational — Studio owns the proposal state and what accept does
 *  to the draft. The expand-on-mount animation lives in .wb-diff-wrap. */
export function LyricsProposal({ proposal, onAccept, onReject }: LyricsProposalProps) {
  const { t } = useI18n();
  return (
    <div className="wb-diff-wrap">
      <div className="wb-diff-inner">
        <div className="wb-diff">
          {proposal.original.map((line, i) => (
            <p key={`old-${i}`} className="wb-diff-old">{line || " "}</p>
          ))}
          {proposal.replacement.map((line, i) => (
            <p key={`new-${i}`} className="wb-diff-new">{line || " "}</p>
          ))}
          <div className="wb-diff-actions">
            <span className="wb-diff-note">{t("proposal_note", { from: proposal.from, to: proposal.to })}</span>
            <button type="button" className="wb-diff-btn" onClick={onAccept}>✓ {t("accept")}</button>
            <button type="button" className="wb-diff-btn reject" onClick={onReject}>✕ {t("reject")}</button>
          </div>
        </div>
      </div>
    </div>
  );
}
