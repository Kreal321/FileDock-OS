// Safe return draft only: connection credentials never enter this module or the URL.
export type DestinationDraft = {
  disk: string;
  connection: string;
  bucket: string;
  prefix: string;
  basic: boolean;
  third: boolean;
};
let draft: DestinationDraft | undefined;
export function saveDestinationDraft(value: DestinationDraft) {
  draft = { ...value };
}
export function takeDestinationDraft() {
  const value = draft;
  draft = undefined;
  return value;
}
export function clearDestinationDraft() {
  draft = undefined;
}
