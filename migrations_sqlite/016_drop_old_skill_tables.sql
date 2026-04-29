-- Drop obsolete skill tables from the old WASM-era skill system.
-- The new OpenClaw-compatible skill system loads skills directly from
-- the filesystem (data/skills/) and no longer uses these tables.
-- skill_ratings is retained as it is still used by SkillRatingStore.

DROP TABLE IF EXISTS agent_skills;
DROP TABLE IF EXISTS skills;
