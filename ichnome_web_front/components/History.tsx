import { IchHistory } from "@/api/types";
import GroupLink from "./GroupLink";
import StatLink from "./StatLink";
import FootprintLink from "./FootprintLink";

type Props = {
  history: IchHistory;
};

export const History: React.FC<Props> = ({
  history: { group_id, path, version, status, mtime, digest, created_at, updated_at },
}) => {
  return (
    <ul>
      <li>
        Group: <GroupLink groupId={group_id} />
      </li>
      <li>
        Path: <StatLink groupId={group_id} path={path} />
      </li>
      <li>Version: {version}</li>
      <li>Status: {status}</li>
      {mtime != null ? <li>File Modified At: {mtime}</li> : undefined}
      {digest != null ? (
        <li>
          Digest: <FootprintLink digest={digest} />
        </li>
      ) : undefined}
      <li>History Created At: {created_at}</li>
      <li>History Updated At: {updated_at}</li>
    </ul>
  );
};

export default History;
