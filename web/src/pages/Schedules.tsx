import { useState, useEffect } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  useRunSchedule,
  useSchedules,
  useToggleSchedule,
  useScheduleDetail,
  useUpdateSchedule,
  useAgents,
  type ScheduleDetail,
} from "@/hooks/use-api";
import { formatDistanceToNow } from "date-fns";
import { AlertTriangle, Clock3, Loader2, Play, Pencil, Save } from "lucide-react";
import { toast } from "sonner";
import { Skeleton } from "@/components/ui/skeleton";
import { ErrorState } from "@/components/ui/error-state";

function UpdatedAgo({ dataUpdatedAt }: { dataUpdatedAt: number }) {
  const [, setTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setTick(t => t + 1), 5000);
    return () => clearInterval(id);
  }, []);
  
  if (!dataUpdatedAt) return null;
  const seconds = Math.floor((Date.now() - dataUpdatedAt) / 1000);
  const text = seconds < 5 ? "just now" : seconds < 60 ? `${seconds}s ago` : `${Math.floor(seconds / 60)}m ago`;
  return <span className="text-xs text-muted-foreground">Updated {text}</span>;
}

function formatSchedule(schedule: {
  kind: "cron" | "at" | "every";
  expr?: string;
  tz?: string;
  at?: string;
  interval_ms?: number;
}) {
  switch (schedule.kind) {
    case "cron":
      return `${schedule.expr ?? "-"} @ ${schedule.tz ?? "UTC"}`;
    case "at":
      return schedule.at ?? "-";
    case "every":
      return `${schedule.interval_ms ?? 0}ms interval`;
    default:
      return "-";
  }
}

function statusVariant(status: "ok" | "error" | "skipped" | null) {
  if (status === "ok") return "text-green-700 border-green-200 bg-green-50";
  if (status === "error") return "text-red-700 border-red-200 bg-red-50";
  if (status === "skipped") return "text-slate-700 border-slate-200 bg-slate-50";
  return "";
}

// ---------------------------------------------------------------------------
// Edit Schedule Dialog
// ---------------------------------------------------------------------------
function EditScheduleDialog({
  scheduleId,
  open,
  onOpenChange,
}: {
  scheduleId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { data: detail, isLoading } = useScheduleDetail(scheduleId);
  const { data: agents } = useAgents();
  const updateMutation = useUpdateSchedule();

  const [form, setForm] = useState<ScheduleDetail | null>(null);

  // Populate form when detail loads
  useEffect(() => {
    if (detail && !form) {
      setForm({ ...detail });
    }
  }, [detail, form]);

  // Reset form when dialog closes
  useEffect(() => {
    if (!open) setForm(null);
  }, [open]);

  const handleSave = async () => {
    if (!form) return;
    try {
      await updateMutation.mutateAsync({ id: scheduleId, data: form });
      toast.success(`Updated: ${form.name}`);
      onOpenChange(false);
    } catch {
      toast.error("Failed to update schedule");
    }
  };

  const updateForm = (patch: Partial<ScheduleDetail>) => {
    if (form) setForm({ ...form, ...patch });
  };

  const updateScheduleType = (kind: "cron" | "at" | "every") => {
    if (!form) return;
    if (kind === "cron") {
      setForm({ ...form, schedule: { kind: "cron", expr: form.schedule.expr ?? "0 9 * * *", tz: form.schedule.tz ?? "UTC" } });
    } else if (kind === "at") {
      setForm({ ...form, schedule: { kind: "at", at: form.schedule.at ?? new Date().toISOString() } });
    } else {
      setForm({ ...form, schedule: { kind: "every", interval_ms: form.schedule.interval_ms ?? 60000 } });
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Edit Schedule</DialogTitle>
          <DialogDescription>Modify schedule configuration. Changes take effect on next run.</DialogDescription>
        </DialogHeader>

        {isLoading || !form ? (
          <div className="space-y-4 py-4">
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-9 w-full" />
          </div>
        ) : (
          <div className="space-y-4 py-2">
            {/* Name */}
            <div className="space-y-1.5">
              <Label htmlFor="sched-name">Name</Label>
              <Input
                id="sched-name"
                value={form.name}
                onChange={(e) => updateForm({ name: e.target.value })}
              />
            </div>

            {/* Description */}
            <div className="space-y-1.5">
              <Label htmlFor="sched-desc">Description</Label>
              <Input
                id="sched-desc"
                value={form.description ?? ""}
                onChange={(e) => updateForm({ description: e.target.value || null })}
                placeholder="Optional description"
              />
            </div>

            {/* Schedule Type */}
            <div className="space-y-1.5">
              <Label>Schedule Type</Label>
              <Select value={form.schedule.kind} onValueChange={(v) => updateScheduleType(v as "cron" | "at" | "every")}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="cron">Cron</SelectItem>
                  <SelectItem value="at">One-time (at)</SelectItem>
                  <SelectItem value="every">Interval (every)</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Cron fields */}
            {form.schedule.kind === "cron" && (
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <Label htmlFor="sched-expr">Cron Expression</Label>
                  <Input
                    id="sched-expr"
                    value={form.schedule.expr ?? ""}
                    onChange={(e) => setForm({ ...form, schedule: { ...form.schedule, expr: e.target.value } })}
                    placeholder="0 9 * * *"
                    className="font-mono text-sm"
                  />
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="sched-tz">Timezone</Label>
                  <Input
                    id="sched-tz"
                    value={form.schedule.tz ?? ""}
                    onChange={(e) => setForm({ ...form, schedule: { ...form.schedule, tz: e.target.value } })}
                    placeholder="Asia/Singapore"
                  />
                </div>
              </div>
            )}

            {/* At field */}
            {form.schedule.kind === "at" && (
              <div className="space-y-1.5">
                <Label htmlFor="sched-at">Run At (ISO 8601)</Label>
                <Input
                  id="sched-at"
                  value={form.schedule.at ?? ""}
                  onChange={(e) => setForm({ ...form, schedule: { ...form.schedule, at: e.target.value } })}
                  placeholder="2026-03-07T10:00:00+08:00"
                  className="font-mono text-sm"
                />
              </div>
            )}

            {/* Every field */}
            {form.schedule.kind === "every" && (
              <div className="space-y-1.5">
                <Label htmlFor="sched-interval">Interval (milliseconds)</Label>
                <Input
                  id="sched-interval"
                  type="number"
                  value={form.schedule.interval_ms ?? 60000}
                  onChange={(e) => setForm({ ...form, schedule: { ...form.schedule, interval_ms: parseInt(e.target.value) || 0 } })}
                />
                <p className="text-xs text-muted-foreground">
                  = {Math.round((form.schedule.interval_ms ?? 0) / 1000 / 60)} minutes
                </p>
              </div>
            )}

            {/* Agent */}
            <div className="space-y-1.5">
              <Label>Agent</Label>
              <Select value={form.agent_id} onValueChange={(v) => updateForm({ agent_id: v })}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(agents ?? []).map((a) => (
                    <SelectItem key={a.agent_id} value={a.agent_id}>
                      {a.emoji} {a.name} ({a.agent_id})
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* Session Mode */}
            <div className="space-y-1.5">
              <Label>Session Mode</Label>
              <Select value={form.session_mode} onValueChange={(v) => updateForm({ session_mode: v as "main" | "isolated" })}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="main">Main</SelectItem>
                  <SelectItem value="isolated">Isolated</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Payload */}
            <div className="space-y-3">
              <Label>Payload</Label>
              <div className="rounded-md border p-3 space-y-3">
                {/* Kind selector */}
                <div className="space-y-1.5">
                  <Label className="text-xs text-muted-foreground">Type</Label>
                  <Select
                    value={form.payload?.kind ?? "agent_turn"}
                    onValueChange={(kind) => {
                      const k = kind as "system_event" | "agent_turn" | "direct_deliver";
                      if (k === "agent_turn") {
                        updateForm({ payload: { kind: k, message: form.payload?.message ?? form.payload?.text ?? "", timeout_seconds: form.payload?.timeout_seconds ?? 300, light_context: form.payload?.light_context ?? false } });
                      } else {
                        updateForm({ payload: { kind: k, text: form.payload?.text ?? form.payload?.message ?? "" } });
                      }
                    }}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="agent_turn">Agent Turn</SelectItem>
                      <SelectItem value="system_event">System Event</SelectItem>
                      <SelectItem value="direct_deliver">Direct Deliver</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                {/* Agent Turn fields */}
                {(form.payload?.kind ?? "agent_turn") === "agent_turn" && (
                  <>
                    <div className="space-y-1.5">
                      <Label htmlFor="sched-message" className="text-xs text-muted-foreground">Message</Label>
                      <textarea
                        id="sched-message"
                        className="flex min-h-[100px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                        value={form.payload?.message ?? ""}
                        onChange={(e) => updateForm({ payload: { ...form.payload!, message: e.target.value } })}
                        placeholder="Task instructions for the agent..."
                      />
                    </div>
                    <div className="grid grid-cols-2 gap-3">
                      <div className="space-y-1.5">
                        <Label htmlFor="sched-model" className="text-xs text-muted-foreground">Model (optional)</Label>
                        <Input
                          id="sched-model"
                          value={form.payload?.model ?? ""}
                          onChange={(e) => updateForm({ payload: { ...form.payload!, model: e.target.value || null } })}
                          placeholder="Use agent default"
                          className="text-sm"
                        />
                      </div>
                      <div className="space-y-1.5">
                        <Label htmlFor="sched-thinking" className="text-xs text-muted-foreground">Thinking (optional)</Label>
                        <Input
                          id="sched-thinking"
                          value={form.payload?.thinking ?? ""}
                          onChange={(e) => updateForm({ payload: { ...form.payload!, thinking: e.target.value || null } })}
                          placeholder="e.g. medium"
                          className="text-sm"
                        />
                      </div>
                    </div>
                    <div className="grid grid-cols-2 gap-3 items-end">
                      <div className="space-y-1.5">
                        <Label htmlFor="sched-payload-timeout" className="text-xs text-muted-foreground">Payload Timeout (s)</Label>
                        <Input
                          id="sched-payload-timeout"
                          type="number"
                          value={form.payload?.timeout_seconds ?? 300}
                          onChange={(e) => updateForm({ payload: { ...form.payload!, timeout_seconds: parseInt(e.target.value) || 300 } })}
                        />
                      </div>
                      <div className="flex items-center gap-2 pb-1">
                        <Switch
                          id="sched-light-ctx"
                          checked={form.payload?.light_context ?? false}
                          onCheckedChange={(checked) => updateForm({ payload: { ...form.payload!, light_context: checked } })}
                        />
                        <Label htmlFor="sched-light-ctx" className="text-xs text-muted-foreground cursor-pointer">Light Context</Label>
                      </div>
                    </div>
                  </>
                )}

                {/* System Event / Direct Deliver fields */}
                {(form.payload?.kind === "system_event" || form.payload?.kind === "direct_deliver") && (
                  <div className="space-y-1.5">
                    <Label htmlFor="sched-text" className="text-xs text-muted-foreground">Text</Label>
                    <textarea
                      id="sched-text"
                      className="flex min-h-[100px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                      value={form.payload?.text ?? ""}
                      onChange={(e) => updateForm({ payload: { ...form.payload!, text: e.target.value } })}
                      placeholder={form.payload?.kind === "direct_deliver" ? "Message to deliver directly..." : "System event text..."}
                    />
                  </div>
                )}
              </div>
            </div>

            {/* Timeout */}
            <div className="space-y-1.5">
              <Label htmlFor="sched-timeout">Timeout (seconds)</Label>
              <Input
                id="sched-timeout"
                type="number"
                value={form.timeout_seconds}
                onChange={(e) => updateForm({ timeout_seconds: parseInt(e.target.value) || 0 })}
              />
            </div>

            {/* Delivery (read-only summary) */}
            {form.delivery && (
              <div className="space-y-1.5">
                <Label>Delivery</Label>
                <div className="rounded-md border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">
                  Mode: <span className="font-medium text-foreground">{form.delivery.mode}</span>
                  {form.delivery.channel && <> · Channel: <span className="font-medium text-foreground">{form.delivery.channel}</span></>}
                  {form.delivery.connector_id && <> · Connector: <span className="font-medium text-foreground">{form.delivery.connector_id}</span></>}
                </div>
                <p className="text-xs text-muted-foreground">Delivery settings are read-only. Edit the YAML directly for advanced changes.</p>
              </div>
            )}

            {/* Enabled */}
            <div className="flex items-center justify-between">
              <Label>Enabled</Label>
              <Switch
                checked={form.enabled}
                onCheckedChange={(enabled) => updateForm({ enabled })}
              />
            </div>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button onClick={handleSave} disabled={updateMutation.isPending || !form}>
            {updateMutation.isPending ? <Loader2 className="h-4 w-4 animate-spin mr-2" /> : <Save className="h-4 w-4 mr-2" />}
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Schedules Skeleton
// ---------------------------------------------------------------------------
function SchedulesSkeleton() {
  return (
    <>
      <div className="flex items-center justify-between mb-6">
        <div>
          <Skeleton className="h-6 w-28" />
          <Skeleton className="h-4 w-48 mt-1" />
        </div>
        <Skeleton className="h-4 w-24" />
      </div>
      <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
        {Array.from({ length: 3 }).map((_, i) => (
          <Card key={i}>
            <CardHeader className="space-y-2">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <Skeleton className="h-5 w-32" />
                  <Skeleton className="h-3 w-24 mt-1" />
                </div>
                <Skeleton className="h-5 w-10" />
              </div>
            </CardHeader>
            <CardContent className="space-y-3">
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-3/4" />
              <Skeleton className="h-9 w-full mt-2" />
            </CardContent>
          </Card>
        ))}
      </div>
    </>
  );
}

export default function SchedulesPage() {
  const { data: schedules, dataUpdatedAt: schedulesUpdatedAt, isLoading, isError, error, refetch } = useSchedules();
  const runMutation = useRunSchedule();
  const toggleMutation = useToggleSchedule();
  const [editId, setEditId] = useState<string | null>(null);

  if (isLoading) return <SchedulesSkeleton />;
  if (isError) return <ErrorState message={error?.message} onRetry={refetch} />

  return (
    <>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-xl font-semibold">Schedules</h1>
          <p className="text-sm text-muted-foreground">Manage your scheduled tasks</p>
        </div>
        <UpdatedAgo dataUpdatedAt={schedulesUpdatedAt} />
      </div>
      <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">

      {(schedules ?? []).map((item) => {
        const nextRunText = item.next_run_at
          ? formatDistanceToNow(new Date(item.next_run_at), { addSuffix: true })
          : "-";

        return (
          <Card key={item.schedule_id} className="group cursor-pointer hover:ring-2 hover:ring-ring/20 transition-all" onClick={() => setEditId(item.schedule_id)}>
            <CardHeader className="space-y-2">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <CardTitle className="text-base">{item.name}</CardTitle>
                  <CardDescription className="mt-1 text-xs">{item.schedule_id}</CardDescription>
                </div>
                <div className="flex items-center gap-2" onClick={(e) => e.stopPropagation()}>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 opacity-0 group-hover:opacity-100 transition-opacity"
                    onClick={() => setEditId(item.schedule_id)}
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </Button>
                  <Switch
                    checked={item.enabled}
                    disabled={toggleMutation.isPending}
                    onCheckedChange={async (enabled) => {
                      try {
                        await toggleMutation.mutateAsync({ id: item.schedule_id, enabled });
                        toast.success(`${enabled ? "Enabled" : "Disabled"}: ${item.name}`);
                      } catch {
                        toast.error(`Failed to update ${item.name}`);
                      }
                    }}
                  />
                </div>
              </div>
              {item.description && <CardDescription>{item.description}</CardDescription>}
            </CardHeader>
            <CardContent className="space-y-3 text-sm">
              <div className="flex items-center gap-2 text-muted-foreground">
                <Clock3 className="h-4 w-4" />
                <span className="truncate">{formatSchedule(item.schedule)}</span>
              </div>

              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">Next run</span>
                <span className="font-medium">{nextRunText}</span>
              </div>

              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">Last status</span>
                <Badge variant="outline" className={statusVariant(item.last_run_status)}>
                  {item.last_run_status ?? "unknown"}
                </Badge>
              </div>

              {item.consecutive_errors > 0 && (
                <div className="flex items-center gap-2 text-amber-700 text-xs bg-amber-50 border border-amber-200 rounded-md px-2 py-1.5">
                  <AlertTriangle className="h-3.5 w-3.5" />
                  Consecutive errors: {item.consecutive_errors}
                </div>
              )}

              <Button
                className="w-full"
                variant="secondary"
                disabled={runMutation.isPending}
                onClick={async (e) => {
                  e.stopPropagation();
                  try {
                    await runMutation.mutateAsync(item.schedule_id);
                    toast.success(`Triggered: ${item.name}`);
                  } catch {
                    toast.error(`Failed to run ${item.name}`);
                  }
                }}
              >
                <Play className="h-4 w-4 mr-2" />
                Run now
              </Button>
            </CardContent>
          </Card>
        );
      })}
      </div>

      {/* Edit Dialog */}
      {editId && (
        <EditScheduleDialog
          scheduleId={editId}
          open={!!editId}
          onOpenChange={(open) => { if (!open) setEditId(null); }}
        />
      )}
    </>
  );
}
