import { IchHistory } from "@/api/types";
import GroupLink from "./GroupLink";
import StatLink from "./StatLink";
import FootprintLink from "./FootprintLink";

type Props = {
  workspaceName: string;
  groupName: string;
  history: IchHistory;
};

export const History: React.FC<Props> = ({
  workspaceName,
  groupName,
  history: { path, version, status, mtime, digest, created_at, updated_at },
}) => {
  return (
    <ul>
      <li>
        Group: <GroupLink workspaceName={workspaceName} groupName={groupName} />
      </li>
      <li>
        Path: <StatLink workspaceName={workspaceName} groupName={groupName} path={path} />
      </li>
      <li>Version: {version}</li>
      <li>Status: {status}</li>
      {mtime != null ? <li>File Modified At: {mtime}</li> : undefined}
      {digest != null ? (
        <li>
          Digest: <FootprintLink workspaceName={workspaceName} digest={digest} />
        </li>
      ) : undefined}
      <li>History Created At: {created_at}</li>
      <li>History Updated At: {updated_at}</li>
    </ul>
  );
};

export default History;
