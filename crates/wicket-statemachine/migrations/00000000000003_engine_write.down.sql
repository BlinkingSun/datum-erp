-- Reverse 0003_engine_write.

DROP FUNCTION IF EXISTS sm.transition_instance(text, uuid, text, text, bigint);
DROP FUNCTION IF EXISTS sm.spawn_instance(text, uuid, uuid, text);
DROP TRIGGER IF EXISTS instance_engine_guard ON sm.instance;
DROP FUNCTION IF EXISTS sm.instance_engine_guard();
DROP VIEW IF EXISTS sm.instance_engine;

GRANT SELECT, INSERT, UPDATE ON TABLE sm.instance TO wicket_app;
