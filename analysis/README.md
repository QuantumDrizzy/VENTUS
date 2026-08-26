# analysis/ — Python. Plots only. QUARANTINED (ADR-000 D8).
# Reads out/**/*.npy and *.csv. Nothing in crates/ or native/ may import or
# invoke anything here. If the build breaks without Python, D8 is violated.
