-- Phase 1.7.11 — Persist the V61.x reference panel alongside each signal so
-- the alert sender and `GET /signals/:symbol` API can replay the full panel
-- (PUTUSAN/TRADE SCORE/BIAS/CONF/SESI/FLOW/EW/TRAP GATE/SL/TP rows) without
-- recomputing from raw candles. The column is nullable so the migration is
-- additive — historic rows simply carry `panel_report = NULL` and the
-- renderer falls back to the legacy short message.

ALTER TABLE signal_evaluations
    ADD COLUMN IF NOT EXISTS panel_report JSONB;
