import csv

def read_rows(path):
    with open(path, newline="") as stream:
        return list(csv.DictReader(stream))
