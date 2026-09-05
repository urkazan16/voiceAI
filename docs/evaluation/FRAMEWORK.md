# Evaluation framework

Weights (100):

| Area                                  | Points |
| ------------------------------------- | -----: |
| Accuracy (STT, formatting, code)      |     25 |
| Speed                                 |     20 |
| Reliability                           |     20 |
| Product completeness                  |     15 |
| Engineering quality                   |     10 |
| Reproducibility / privacy / licensing |     10 |

Passing: total ≥ 85, Accuracy ≥ 20, Speed ≥ 15, Reliability ≥ 17.

Critical automatic failures: not offline, audio leaving the machine, non-local LLM, frequent crashes, unbuildable tree, missing LICENSE, unexplained proprietary dependency, model load without checksum, generated code executed.
