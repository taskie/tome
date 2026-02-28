import { IchGroup } from "@/api/types";
import GroupLink from "./GroupLink";
import FootprintLink from "./FootprintLink";

type Props = {
  workspaceName: string;
  group: IchGroup;
};

export const Group: React.FC<Props> = ({
  workspaceName,
  group: { id, name, type, url, digest, created_at, updated_at },
}) => {
  return (
    <ul>
      <li>
        ID: <GroupLink workspaceName={workspaceName} groupName={name} /> (Definition:{" "}
        <GroupLink workspaceName={workspaceName} groupName={name} family="groups" />)
      </li>
      <li>Type: {type}</li>
      <li>URL: {url}</li>
      {digest != null ? (
        <li>
          Digest: <FootprintLink workspaceName={workspaceName} digest={digest} />
        </li>
      ) : undefined}
      <li>Group Created At: {created_at}</li>
      <li>Group Updated At: {updated_at}</li>
    </ul>
  );
};

export default Group;
