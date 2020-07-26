import { NextPage } from "next";
import Head from "next/head";
import { useRouter } from "next/router";
import { uria } from "@/utils/uri";
import { defaultInstance } from "@/api/apiClient";
import { applicationName } from "@/config";
import { GetStatsResponse } from "@/api/types";
import Group from "@/components/Group";
import StatGroup from "@/components/StatGroup";

type Query = {
  workspaceName: string;
  groupName: string;
};

type Response = GetStatsResponse;

type Props = { response?: Response; err?: string };

const ResponseView: React.FC<{ response: Response; workspaceName: string; groupName: string }> = ({
  response: { group, stats },
  workspaceName,
  groupName,
}) => {
  return (
    <>
      <h2>Stats</h2>
      <StatGroup workspaceName={workspaceName} groupName={groupName} stats={stats} />
      <h2>Group</h2>
      <Group workspaceName={workspaceName} group={group} />
    </>
  );
};

export const StatsPage: NextPage<Props> = (props) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const { workspaceName, groupName } = (rawQuery as unknown) as Query;
  return (
    <div className="container">
      <Head>
        <title>
          Stats of {groupName} - {applicationName}
        </title>
      </Head>
      <h1>Stats of {groupName}</h1>
      {props.response != null ? (
        <ResponseView response={props.response} workspaceName={workspaceName} groupName={groupName} />
      ) : (
        <p>Some error occured: {props.err}</p>
      )}
    </div>
  );
};

StatsPage.getInitialProps = async ({ query: rawQuery }) => {
  try {
    const { workspaceName, groupName } = (rawQuery as unknown) as Query;
    const path = uria`${workspaceName}/stats/${groupName}`;
    const { data } = await defaultInstance.get(path);
    return { response: data };
  } catch (err) {
    console.error(err);
    return { err: err.message };
  }
};

export default StatsPage;
