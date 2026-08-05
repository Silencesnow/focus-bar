import { readFocusData, setGlobalNote, writeFocusData } from "./store";

export type GlobalNoteSaveState = "pending" | "saving" | "saved" | "error";

type SaveStateListener = (state: GlobalNoteSaveState) => void;
type PendingSave = { note: string; onState: SaveStateListener };

export async function saveGlobalNote(note: string): Promise<void> {
  const data = await readFocusData();
  await writeFocusData(setGlobalNote(data, note));
}

export function createGlobalNoteAutosave(
  save: (note: string) => Promise<void>,
  delay = 300,
) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending: PendingSave | null = null;
  let active: Promise<void> | null = null;

  function clearTimer() {
    if (timer) clearTimeout(timer);
    timer = null;
  }

  async function persistPending(): Promise<void> {
    if (active) await active;
    const current = pending;
    pending = null;
    if (!current) return;
    current.onState("saving");
    active = save(current.note)
      .then(() => current.onState("saved"))
      .catch(() => current.onState("error"));
    await active;
    active = null;
  }

  return {
    schedule(note: string, onState: SaveStateListener) {
      clearTimer();
      pending = { note, onState };
      onState("pending");
      timer = setTimeout(() => {
        timer = null;
        void persistPending();
      }, delay);
    },
    async flush() {
      clearTimer();
      while (pending || active) await persistPending();
    },
    cancel() {
      clearTimer();
      pending = null;
    },
  };
}
