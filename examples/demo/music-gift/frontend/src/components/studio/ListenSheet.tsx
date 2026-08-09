import { AnimatePresence, motion, useReducedMotion, type PanInfo } from "motion/react";
import type { ReactNode } from "react";

export interface ListenSheetProps {
  open: boolean;
  onClose: () => void;
  /** The artifact column's content (PlayerCard + TakesCard + AI chat panel),
   *  rendered here on mobile instead of in a hidden second column. */
  children: ReactNode;
}

/** Velocity (px/s) past which a release counts as a downward fling, and the
 *  slow-drag distance past which the sheet closes anyway. */
const CLOSE_VELOCITY = 300;
const CLOSE_OFFSET = 140;

/** Mobile bottom sheet (Task 11): the artifact column's content pulled up
 *  over the driver column. Spring open/close per the spring-sheet preset
 *  ({ type: "spring", bounce: 0.2, duration: 0.35 }); release velocity sign
 *  decides expand/collapse; the dim backdrop closes on tap. Reduced motion
 *  degrades to a fade (no drag). The sheet stays mounted while closed so
 *  the audio element inside PlayerCard keeps playing behind the dock. */
export function ListenSheet({ open, onClose, children }: ListenSheetProps) {
  const reduced = useReducedMotion();

  function handleDragEnd(_: unknown, info: PanInfo) {
    if (info.velocity.y > CLOSE_VELOCITY || info.offset.y > CLOSE_OFFSET) onClose();
  }

  return (
    <>
      <AnimatePresence>
        {open && (
          <motion.div
            className="wb-dim"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            onClick={onClose}
          />
        )}
      </AnimatePresence>
      <motion.div
        className="wb-sheet"
        initial={false}
        animate={
          reduced
            ? { opacity: open ? 1 : 0 }
            : { y: open ? 0 : "100%" }
        }
        transition={
          reduced
            ? { duration: 0.15 }
            : { type: "spring", bounce: 0.2, duration: 0.35 }
        }
        style={{ pointerEvents: open ? "auto" : "none" }}
        drag={reduced || !open ? false : "y"}
        dragConstraints={{ top: 0, bottom: 0 }}
        dragElastic={{ top: 0, bottom: 0.5 }}
        onDragEnd={handleDragEnd}
        inert={!open}
      >
        <div className="wb-sheet-grab" />
        <div className="wb-sheet-body">{children}</div>
      </motion.div>
    </>
  );
}
