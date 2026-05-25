Research question:
{{question}}

Current date: {{today}}
Report path: {{report_path}}

Run a real deep-research workflow before answering. Use web_research as an isolated sub-agent; each call should delegate exactly one research angle.

Required phases:
1. Broad exploration
   - Call web_research for an initial landscape survey.
   - Call web_research again to identify dimensions, stakeholders, or schools of thought.
2. Targeted deep dives
   - Choose the most important dimensions.
   - Call web_research separately for concrete data/statistics, examples or case studies, and expert/authoritative views.
3. Diversity and validation
   - Call web_research for challenges, limitations, criticism, or conflicting evidence.
   - If the topic is current, include a recency-focused query using {{today}}.
4. Synthesis check
   - Do not write the final report until you have at least {{min_calls}} web_research calls unless the question is clearly too narrow. If you use fewer, explicitly justify why in the report.
   - Verify coverage includes facts/data, examples, expert or authoritative sources, trends/current context, and limitations.

Write a markdown report to {{report_path}} using write_file. The report must include:
- Executive summary
- Research method: list the search angles used
- Key findings with citations as URLs
- Evidence table: claim, source URL, date, confidence
- Limitations / contradictory evidence
- Remaining open questions
- Final answer

After write_file succeeds, return a concise final message with the report path and the most important source URLs.
