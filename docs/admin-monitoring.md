# Live administration monitoring

Open **Site administration → Dashboard** to see request traffic, average response time,
server error rate, HTTP status classes, and smart-HTTP clone/fetch and push activity.
Only signed-in site administrators can read the dashboard or its monitoring endpoint.

The panel refreshes every five seconds and shows up to sixty samples (approximately
five minutes). Tables summarize the same collected window. A new process needs five
seconds to collect its first sample; history resets on restart and is local to that
process. With multiple server processes, use a sticky admin session to follow one
process, or Prometheus for aggregated, persistent monitoring.

Response time is the average time until response headers are ready, including the
middleware. Streamed Git packs and downloads can continue after that point. A gap in
the response-time chart means no requests completed in that sample, rather than a
zero-millisecond response. The error rate counts **5xx** responses; **4xx** responses
appear separately in the table. Git counts smart-HTTP pack-exchange responses when
their headers are ready, including failed responses, excluding reference advertisements
and SSH traffic. Transfer bodies may still be streaming when an exchange is counted.
Monitoring polls and Prometheus scrapes are excluded from the panel's statistics.

**Pause** freezes the current display; **Resume** reconnects immediately. Updates stop
when the browser tab is hidden or the dashboard is left. Failed connections keep the
last successful snapshot and retry with a delay of up to sixty seconds. A session or
permission change stops polling and asks you to reload. Without JavaScript, the initial
snapshot is still available; reload to update it.

Collection adds only fixed-size atomic counters to the existing HTTP instrumentation.
A single background sampler retains at most sixty aggregate records. Requests never
lock the history, store request paths, query the database, or traverse the data directory
for monitoring. As with other counter-based telemetry, independent atomic reads can
slightly shift counts and duration between adjacent samples under concurrency. SVG
charts render on the server using the site's semantic theme colors; a small module is
loaded only on the dashboard. Each uncached refresh replaces only the monitoring
fragment and rechecks the administrator's session. Instance inventory tiles and system
information are refreshed with the full page, outside the live sampling loop.

This panel is available independently of `[operations] metrics`. Keep using `/metrics`
and your configured Prometheus token for long-term retention, alerting, background-job
metrics, and analysis across instances.

## Restarting the application

The **Restart server** button sits opposite **Dashboard** in the page heading and is
also available in the **Configuration** heading, beside the saved deployment settings. It uses
the site's normal button styling and follows every theme. It opens a confirmation page;
only the confirmed, same-origin administrator POST requests a restart. The request is
recorded as **Requested a server restart** in the administration audit log. Repeated
requests are coalesced into the same shutdown.

With JavaScript, the confirmation page loads its reconnect logic before sending the
restart request and retains that document during shutdown. Reconnection does not depend
on downloading a new script while the server is offline. An unconfirmed response is
never automatically retried as another restart request.

rbit finishes active HTTP requests, stops its SSH listener and background jobs, closes
storage, and shuts down its runtime before relaunching the installed executable with
the original arguments, working directory and environment. Long-running HTTP transfers
can delay the restart; SSH connections close. The new application rereads the configured
TOML file and saved deployment settings. Persisted users, repositories and sessions remain
available, and the reconnect page returns the administrator to the dashboard once the
new application is ready. Without JavaScript, use **Return to dashboard** after waiting.

On Unix the executable replaces the existing process, preserving the PID tracked by
systemd and container runtimes. On Windows the original process supervises replacement
workers, keeping the service PID stable and the process count bounded across repeated
restarts. Workers reuse an existing console; headless servers open no additional window.
An embedding host that does not subscribe to restart
requests shows the control as unavailable; use that host's service manager instead.
For multiple replicas the action restarts only the process receiving the request, so
use a sticky administrator session when controlling a particular server.
