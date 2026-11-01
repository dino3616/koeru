/**
 * 棚とパンくずに出す音源の名前。 manifest が読めない音源は名前が空で届くので、
 * 空のまま並べない（`Avatar` の名前と同じ言い方）。
 */
export const voiceName = (name: string): string => (name.trim() === "" ? "名前の無い音源" : name);
