// Contact names for chat handles (phone numbers / emails), resolved on the Mac.
import { useCallback } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "./bb";
import { useApp } from "./app";

export type NameOf = (handle: string) => string;

/** `name(handle)`: the contact name, or the handle itself when there's no contact. */
export function useNames(): NameOf {
  const { connected } = useApp();
  const { data } = useQuery({ queryKey: ["contacts"], queryFn: api.contacts, enabled: connected, staleTime: 10 * 60_000 });
  return useCallback((handle: string) => data?.get(handle) ?? handle, [data]);
}
