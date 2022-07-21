from concurrent import futures
import datetime as dt
import os
import os.path
import re
import time

# import joblib
from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.linear_model import LogisticRegression
import pandas as pd

from google.protobuf.duration_pb2 import Duration
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc

from minio import Minio


MINIO_ADDRESS = os.getenv("MINIO_ADDRESS")
ACCESS_KEY = "minioroot"
SECRET_KEY = "minioroot"
BUCKET_NAME = "fbpml"

TMPFS_MOUNTPOINT = "/writable_tmpfs"
DF_NAMES = ["dataset1.csv", "dataset2.csv"]
DF_PATHS = list(map(lambda n: os.path.join(TMPFS_MOUNTPOINT, n), DF_NAMES))


cleanup_re = re.compile("[^a-z]+")


def cleanup(sentence):
    sentence = sentence.lower()
    sentence = cleanup_re.sub(" ", sentence).strip()
    return sentence


class LRTraining(fbpml_grpc.OneArgumentServicer):
    def Bench(
        self, request: fbpml.OneArgumentRequest, context: grpc.ServicerContext
    ):
        response_start = time.time()
        response_duration = Duration()

        idx = request.arg if request.arg and 0 <= request.arg <= 1 else 0

        # Download the input dataset from MinIO
        minio_client = Minio(
            MINIO_ADDRESS,
            access_key=ACCESS_KEY,
            secret_key=SECRET_KEY,
            secure=False,
        )
        minio_client.fget_object(BUCKET_NAME, DF_NAMES[idx], DF_PATHS[idx])

        df = pd.read_csv(DF_PATHS[idx])
        df["train"] = df["Text"].apply(cleanup)
        tfidf_vector = TfidfVectorizer(min_df=100).fit(df["train"])
        train = tfidf_vector.transform(df["train"])
        model = LogisticRegression()
        model.fit(train, df["Score"])

        # joblib.dump(model, "/bench/lr_model.pk")
        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_OneArgumentServicer_to_server(LRTraining(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
