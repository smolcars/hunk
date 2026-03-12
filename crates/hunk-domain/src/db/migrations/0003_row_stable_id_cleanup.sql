UPDATE comments
SET row_stable_id = NULL
WHERE row_stable_id < 0;
