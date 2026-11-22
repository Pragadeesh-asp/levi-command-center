-- Migration 010: token usage, latency and tool mode per AI request.
-- Additive only: existing ai_requests rows keep NULL in every new column.
-- The provider reports token counts; LEVI never estimates them.

ALTER TABLE ai_requests ADD COLUMN prompt_tokens INTEGER CHECK(prompt_tokens IS NULL OR prompt_tokens >= 0);
ALTER TABLE ai_requests ADD COLUMN completion_tokens INTEGER CHECK(completion_tokens IS NULL OR completion_tokens >= 0);
ALTER TABLE ai_requests ADD COLUMN total_tokens INTEGER CHECK(total_tokens IS NULL OR total_tokens >= 0);
ALTER TABLE ai_requests ADD COLUMN latency_ms INTEGER CHECK(latency_ms IS NULL OR latency_ms >= 0);
ALTER TABLE ai_requests ADD COLUMN finish_reason TEXT;
ALTER TABLE ai_requests ADD COLUMN tool_mode TEXT CHECK(tool_mode IS NULL OR tool_mode IN ('native','text'));
