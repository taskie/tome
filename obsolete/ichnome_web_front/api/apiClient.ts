import { AxiosRequestConfig } from "axios";
import { apiEndpointURL } from "../config";
import axios from "axios";
export const endpointURL = apiEndpointURL;
export const defaultRequestConfig: AxiosRequestConfig = { baseURL: endpointURL };
export const defaultInstance = axios.create(defaultRequestConfig);
/*
defaultInstance.interceptors.request.use((request) => {
  console.log("Starting Request: ", request);
  return request;
});

defaultInstance.interceptors.response.use((response) => {
  console.log("Response: ", response);
  return response;
});
*/
