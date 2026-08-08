import { useI18n } from "../../i18n";

/** Scoped edit commands for the lyric selection toolbar. The fixed
 *  commands fire a scoped chat turn immediately; "custom" routes to the
 *  chat input with the range prefilled (Studio owns both paths). */
export type ScopedCommand = "rewrite" | "rhyme" | "colloquial" | "shorten" | "custom";

const COMMANDS: ScopedCommand[] = ["rewrite", "rhyme", "colloquial", "shorten", "custom"];

export interface SelectionToolbarProps {
  onAction: (cmd: ScopedCommand) => void;
}

/** Dark pill floating above the selected manuscript lines. Pure
 *  presentational — Studio owns visibility, positioning and the action
 *  handling. */
export function SelectionToolbar({ onAction }: SelectionToolbarProps) {
  const { t } = useI18n();
  return (
    <div className="wb-seltoolbar" role="toolbar">
      {COMMANDS.map(cmd => (
        <button
          key={cmd}
          type="button"
          className={cmd === "custom" ? "accent" : undefined}
          // Keep the textarea selection alive until the click handler runs:
          // the blur-clear delay in Studio would otherwise race the click.
          onMouseDown={e => e.preventDefault()}
          onClick={() => onAction(cmd)}
        >
          {t(`scoped_${cmd}`)}
        </button>
      ))}
    </div>
  );
}
