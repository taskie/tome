import { NextPage } from "next";
import Head from "next/head";
import { useRouter } from "next/router";
import { uria } from "@/utils/uri";
import { defaultInstance } from "@/api/apiClient";
import { applicationName } from "@/config";
import { GetStatsResponse } from "@/api/types";
import Group from "@/components/Group";
import StatGroup from "@/components/StatGroup";
import { useForm } from "react-hook-form";
import Link from "next/link";
import { useEffect } from "react";
import { rejectEmpty } from "@/utils/record";
import GlobalNav from "@/components/GlobalNav";

type FormData = {
  path_prefix?: string;
  status?: string;
  mtime_after?: string;
  mtime_before?: string;
  updated_at_after?: string;
  updated_at_before?: string;
};

type Query = {
  workspaceName: string;
  groupName: string;
} & FormData;

type Response = GetStatsResponse;

type Props = { response?: Response; err?: string };

const PagenationView: React.FC<{
  workspaceName: string;
  groupName: string;
  formData?: FormData;
  updated_at_before: string;
}> = ({ workspaceName, groupName, formData, updated_at_before }) => {
  const query = rejectEmpty({ ...formData, updated_at_before });
  const href = { pathname: "/[workspaceName]/stats/[groupName]", query };
  const as = { pathname: uria`/${workspaceName}/stats/${groupName}`, query };
  return (
    <>
      <div>
        <Link href={href} as={as}>
          <a>&raquo; Select Older (Updated At Before: {updated_at_before})</a>
        </Link>
      </div>
    </>
  );
};

const ResponseView: React.FC<{
  response: Response;
  workspaceName: string;
  groupName: string;
  formData?: FormData;
}> = ({ response: { group, stats, stats_count }, workspaceName, groupName, formData }) => {
  const pagenation =
    stats.length > 0 ? (
      <PagenationView
        workspaceName={workspaceName}
        groupName={groupName}
        formData={formData}
        updated_at_before={stats[stats.length - 1].updated_at}
      />
    ) : undefined;
  return (
    <>
      <h2>Stats</h2>
      <dl>
        <dt>Count:</dt>
        <dd>{stats_count}</dd>
      </dl>
      {pagenation}
      <StatGroup workspaceName={workspaceName} groupName={groupName} stats={stats} />
      {pagenation}
      <h2>Group</h2>
      <Group workspaceName={workspaceName} group={group} />
    </>
  );
};

type StatsFormProps = {
  formData: FormData;
  onSubmit: (form: FormData) => void;
};

export const StatsForm: React.FC<StatsFormProps> = ({ onSubmit, formData }) => {
  const { register, handleSubmit, reset } = useForm<FormData>();
  useEffect(() => {
    reset(formData);
  }, [reset, formData]);
  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <dl>
        <dt>
          <label>Path Prefix:</label>
        </dt>
        <dd>
          <input type="text" name="path_prefix" placeholder="data/archives" size={80} ref={register} />
        </dd>
        <dt>
          <label>Status:</label>
        </dt>
        <dd>
          <select name="status" ref={register}>
            <option value="">(None)</option>
            <option value="disabled">Disabled</option>
            <option value="enabled">Enabled</option>
          </select>
        </dd>
        <dt>
          <label>File Modified At:</label>
        </dt>
        <dd>
          <input type="text" name="mtime_after" placeholder="YYYY-mm-ddTHH:MM:SSZ" ref={register} />
          {" - "}
          <input type="text" name="mtime_before" placeholder="YYYY-mm-ddTHH:MM:SSZ" ref={register} />
        </dd>
        <dt>
          <label>Updated At:</label>
        </dt>
        <dd>
          <input type="text" name="updated_at_after" placeholder="YYYY-mm-ddTHH:MM:SSZ" ref={register} />
          {" - "}
          <input type="text" name="updated_at_before" placeholder="YYYY-mm-ddTHH:MM:SSZ" ref={register} />
        </dd>
      </dl>
      <button>Select</button>
    </form>
  );
};

export const StatsPage: NextPage<Props> = (props) => {
  const router = useRouter();
  const { query: rawQuery } = router;
  const {
    workspaceName,
    groupName,
    path_prefix,
    status,
    mtime_after,
    mtime_before,
    updated_at_after,
    updated_at_before,
  } = (rawQuery as unknown) as Query;
  const pageTitle = `Group: ${groupName}`;
  const formData: FormData = { path_prefix, status, mtime_after, mtime_before, updated_at_after, updated_at_before };
  const changeUrl = (data: FormData) => {
    const query = rejectEmpty(data);
    const href = { pathname: "/[workspaceName]/stats/[groupName]", query };
    const as = { pathname: uria`/${workspaceName}/stats/${groupName}`, query };
    router.push(href, as);
  };
  return (
    <div className="container">
      <Head>
        <title>
          {pageTitle} - {applicationName}
        </title>
      </Head>
      <GlobalNav workspaceName={workspaceName} />
      <h1>{pageTitle}</h1>
      <StatsForm formData={formData} onSubmit={changeUrl} />
      {props.response != null ? (
        <ResponseView
          response={props.response}
          workspaceName={workspaceName}
          groupName={groupName}
          formData={formData}
        />
      ) : (
        <p>Some error occured: {props.err}</p>
      )}
    </div>
  );
};

StatsPage.getInitialProps = async ({ query: rawQuery }) => {
  try {
    const {
      workspaceName,
      groupName,
      path_prefix,
      status,
      mtime_after,
      mtime_before,
      updated_at_after,
      updated_at_before,
    } = (rawQuery as unknown) as Query;
    const path = uria`${workspaceName}/stats/${groupName}`;
    const { data } = await defaultInstance.get(path, {
      params: { path_prefix, status, mtime_after, mtime_before, updated_at_after, updated_at_before },
    });
    return { response: data };
  } catch (err) {
    // console.error(err);
    return { err: err.message };
  }
};

export default StatsPage;
