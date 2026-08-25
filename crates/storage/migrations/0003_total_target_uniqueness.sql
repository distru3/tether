-- Total orders must be unique too.
--
-- The `UNIQUE (target_type, target_id)` constraints from 0001/0002 never
-- fire for `target_type = 'total'`: SQL UNIQUE treats NULLs as distinct from
-- everything, including other NULLs, and a total order stores
-- `target_id = NULL`. Every suspend/edit of the total order therefore
-- INSERTed a fresh row instead of updating. These expression indexes make
-- NULL participate (`IFNULL(target_id, -1)`), after collapsing any duplicate
-- rows already written (keeping the oldest per target).

DELETE FROM limits
WHERE id NOT IN (
    SELECT MIN(id) FROM limits GROUP BY target_type, IFNULL(target_id, -1)
);

CREATE UNIQUE INDEX limits_target_unique
    ON limits (target_type, IFNULL(target_id, -1));

DELETE FROM pending_limits
WHERE id NOT IN (
    SELECT MIN(id) FROM pending_limits GROUP BY target_type, IFNULL(target_id, -1)
);

CREATE UNIQUE INDEX pending_limits_target_unique
    ON pending_limits (target_type, IFNULL(target_id, -1));
