ALTER TABLE job_definitions
ADD COLUMN python_dependencies TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[];
