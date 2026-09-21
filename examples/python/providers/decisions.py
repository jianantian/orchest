"""Classify one customer-support case with OpenRouter Decisions.

Requires: OPENROUTER_API_KEY
"""

from orchest import decide

result = decide(
    model="openrouter/~typesafe/jev-latest",
    api_key_env="OPENROUTER_API_KEY",
    state={
        "message": "I was charged twice and need this fixed today.",
        "customer": {"plan": "pro", "open_cases": 0},
    },
    questions={
        "urgent": {
            "type": "boolean",
            "instructions": "Does this need urgent handling?",
        },
        "team": {
            "type": "choice",
            "instructions": "Which team should own the case?",
            "criteria": {"billing": "Payment issues", "support": "Product help"},
        },
        "severity": {
            "type": "score",
            "instructions": "Rate customer impact.",
            "criteria": ["Low", "Moderate", {"label": "High", "escalate": True}],
        },
    },
)

urgent = result["answers"]["urgent"]["probability"] >= 0.8
severity = result["answers"]["severity"]["score"]
confidence = result["answers"]["team"].get("confidence")
if urgent or severity >= 1.5 or (confidence is not None and confidence < 0.6):
    print("Escalate to a human")
print(result)
