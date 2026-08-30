import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

/** Tailwind のクラスを、後勝ちで正しく畳んで繋ぐ。 */
export const cn = (...inputs: ClassValue[]) => twMerge(clsx(inputs));
