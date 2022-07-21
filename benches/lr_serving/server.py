from concurrent import futures
import datetime as dt
import re
import time

# from sklearn.externals import joblib
import joblib
from sklearn.feature_extraction.text import TfidfVectorizer
import pandas as pd

from google.protobuf.duration_pb2 import Duration
import grpc

import functionbench_pmem_local_pb2 as fbpml
import functionbench_pmem_local_pb2_grpc as fbpml_grpc


cleanup_re = re.compile("[^a-z]+")


def cleanup(sentence):
    sentence = sentence.lower()
    sentence = cleanup_re.sub(" ", sentence).strip()
    return sentence


dataset = pd.read_csv("/bench/dataset.csv")
df_input = pd.DataFrame()
dataset["train"] = dataset["Text"].apply(cleanup)
tfidf_vect = TfidfVectorizer(min_df=100).fit(dataset["train"])

x = "The ambiance is magical. The food and service was nice! The lobster and cheese was to die for and our steaks were cooked perfectly.  "
df_input["x"] = [x]
df_input["x"] = df_input["x"].apply(cleanup)
X1 = tfidf_vect.transform(df_input["x"])

x = "My favorite cafe. I like going there on weekends, always taking a cafe and some of their pastry before visiting my parents.  "
df_input["x"] = [x]
df_input["x"] = df_input["x"].apply(cleanup)
X2 = tfidf_vect.transform(df_input["x"])

Xs = [X1, X2]

model = joblib.load("/bench/lr_model.pk")
print("Model is ready")


class LRServing(fbpml_grpc.OneArgumentServicer):
    def Bench(
        self, request: fbpml.OneArgumentRequest, context: grpc.ServicerContext
    ):
        response_start = time.time()
        response_duration = Duration()

        idx = request.arg if request.arg and 0 <= request.arg <= 1 else 0
        _ = model.predict(Xs[idx])

        response_duration.FromTimedelta(
            dt.timedelta(seconds=time.time() - response_start)
        )
        return fbpml.ServiceResponse(response_duration=response_duration)


def serve():
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    fbpml_grpc.add_OneArgumentServicer_to_server(LRServing(), server)
    server.add_insecure_port("[::]:50051")
    server.start()
    server.wait_for_termination()


if __name__ == "__main__":
    serve()
