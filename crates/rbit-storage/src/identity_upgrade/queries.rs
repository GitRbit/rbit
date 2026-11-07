//! Fixed bounded projections: no PHCs, names, addresses or user text leave SQLite.
pub(super) const STATE: &str = "SELECT
 CASE mode WHEN 'legacy' THEN 1 WHEN 'preparing' THEN 2 WHEN 'prepared' THEN 3 ELSE 0 END AS mode,
 CASE WHEN generation IS NULL THEN 0 WHEN typeof(generation)='blob' AND length(generation)=16 THEN 1 ELSE 2 END AS generation_shape,
 CASE WHEN pair IS NULL THEN 0 WHEN typeof(pair)='blob' AND length(pair)=16 THEN 1 ELSE 2 END AS pair_shape,
 CASE WHEN typeof(generation)='blob' AND length(generation)=16 THEN generation END AS generation,
 CASE WHEN typeof(pair)='blob' AND length(pair)=16 THEN pair END AS pair,
 observed_floor AS floor, retired_intent AS retired, accounts, final_floor
 FROM credential_protocol WHERE singleton=1
 AND (SELECT count(*) FROM (SELECT 1 FROM credential_protocol LIMIT 2))=1 LIMIT 2";

pub(super) const HISTORY: &str = "WITH
 alloc AS (
  SELECT count(*) AS n, COALESCE(MAX(high_water),-1) AS high,
   COALESCE(MIN(typeof(singleton)='integer' AND singleton=1 AND typeof(high_water)='integer' AND high_water>=0),0) AS valid
  FROM (SELECT singleton,high_water FROM credential_epoch_allocator LIMIT 2)
 ),
 logins AS (
  SELECT count(*) AS n, COALESCE(MAX(authentication_epoch),0) AS high,
   COALESCE(MIN(typeof(authentication_epoch)='integer' AND authentication_epoch>=0 AND account_id>0),1) AS valid
  FROM user_logins
 ),
 revocations AS (
  SELECT COALESCE(MAX(id),0) AS last_id, COALESCE(MAX(target_epoch),0) AS target,
   COALESCE(MIN(typeof(id)='integer' AND id>0 AND typeof(target_epoch)='integer' AND target_epoch>0),1) AS valid
  FROM credential_revocations
 ),
 sequence AS (
  SELECT count(*) AS n, COALESCE(MAX(seq),0) AS high,
   COALESCE(MIN(typeof(seq)='integer' AND seq>0),1) AS valid
  FROM (SELECT seq FROM sqlite_sequence WHERE name='credential_revocations' LIMIT 2)
 )
 SELECT MAX(alloc.high,logins.high,revocations.target,sequence.high) AS floor,
 revocations.last_id AS retired, logins.n AS accounts,
 (alloc.n=1 AND alloc.valid AND logins.valid AND revocations.valid AND sequence.valid
  AND sequence.n<=1 AND sequence.high>=revocations.last_id
  AND MAX(alloc.high,sequence.high)>=MAX(logins.high,revocations.target)
  AND NOT EXISTS(SELECT 1 FROM user_logins l LEFT JOIN accounts a ON a.id=l.account_id
                 WHERE a.id IS NULL OR a.kind<>'user')) AS valid
 FROM alloc,logins,revocations,sequence";

pub(super) const PAIR: &str = "SELECT CASE WHEN singleton=1 AND
 typeof(identity)='blob' AND
 length(identity)=16 THEN identity END FROM
 credential_session_pair LIMIT 2";

pub(super) const WRITE_PREPARING: &str =
    "UPDATE credential_protocol SET mode='preparing',generation=?,pair=?,observed_floor=?,retired_intent=?,accounts=? WHERE
 singleton=1 AND
 mode='legacy'";

pub(super) const SET_PREPARED: &str = "UPDATE credential_protocol SET mode='prepared',final_floor=? WHERE
 singleton=1 AND
 mode='preparing' AND
 generation=?";
