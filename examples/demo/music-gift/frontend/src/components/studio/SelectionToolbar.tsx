import { useI18n } from "../../i18n";

/** Scoped edit commands for the lyric selection toolbar. The AI wiring
 *  lands in a later task — for now only "custom" has an effect (it prefills
 *  the chat input); the rest are inert placeholders. */
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
