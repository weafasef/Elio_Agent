// ── Bridge stub — remote-control feature removed ────

export class BoundedUUIDSet {
  constructor(_maxSize?: number) {}
  add(_id: string): boolean { return false }
  has(_id: string): boolean { return false }
  delete(_id: string): boolean { return false }
  clear(): void {}
  get size(): number { return 0 }
}
