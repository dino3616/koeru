import type { ShelfVoice } from "~/components/app-shell/sidebar";
import type { SubbankItem } from "~/components/app-shell/subbank";

/*
 * story が共有する作り話。 ボード（S06-Voice）の棚と木に合わせてある。
 * 識別子は UUID の形にしておく——本物の経路は `id` を UUID として検証する。
 */

export const NAGI = "11111111-1111-4111-8111-111111111111";
export const HARUKAZE = "22222222-2222-4222-8222-222222222222";
export const NEW_VOICE_2 = "33333333-3333-4333-8333-333333333333";

export const 棚_ひとつ: readonly ShelfVoice[] = [
  { id: NAGI, name: "ナギ", color: { hue: 175, chroma: 0.6, lightness: 0.5 } },
];

export const 棚_三つ: readonly ShelfVoice[] = [
  ...棚_ひとつ,
  { id: HARUKAZE, name: "ハルカゼ", color: { hue: 290, chroma: 0.6, lightness: 0.5 } },
  // まだ1つも録れていない音源。 色が無く、顔は「新」になる。
  { id: NEW_VOICE_2, name: "新しい音源 2", color: null },
];

export const 木: readonly SubbankItem[] = [
  { id: "sb-normal-a3", voiceColor: "Normal", tone: "A3", method: "CVVC" },
  { id: "sb-normal-g4", voiceColor: "Normal", tone: "G4", method: "CVVC" },
  { id: "sb-whisper-a3", voiceColor: "Whisper", tone: "A3", method: "CVVC" },
  { id: "sb-whisper-g4", voiceColor: "Whisper", tone: "G4", method: "CVVC" },
  { id: "sb-whisper-c5", voiceColor: "Whisper", tone: "C5", method: "SEQUENTIAL" },
  { id: "sb-growl-a3", voiceColor: "Growl", tone: "A3", method: "SINGLE" },
  { id: "sb-growl-c5", voiceColor: "Growl", tone: "C5", method: "SINGLE" },
];
